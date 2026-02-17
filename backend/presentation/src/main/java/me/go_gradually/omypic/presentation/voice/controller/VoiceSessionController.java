package me.go_gradually.omypic.presentation.voice.controller;

import jakarta.validation.Valid;
import me.go_gradually.omypic.application.voice.model.VoiceAudioChunkCommand;
import me.go_gradually.omypic.application.voice.model.VoiceEventSink;
import me.go_gradually.omypic.application.voice.model.VoiceSessionOpenCommand;
import me.go_gradually.omypic.application.voice.model.VoiceSessionStopCommand;
import me.go_gradually.omypic.application.voice.usecase.VoiceSessionUseCase;
import me.go_gradually.omypic.presentation.voice.dto.VoiceAudioChunkRequest;
import me.go_gradually.omypic.presentation.voice.dto.VoiceSessionCreateRequest;
import me.go_gradually.omypic.presentation.voice.dto.VoiceSessionCreateResponse;
import me.go_gradually.omypic.presentation.voice.dto.VoiceSessionRecoveryResponse;
import me.go_gradually.omypic.presentation.voice.dto.VoiceSessionStopRequest;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;

import java.io.IOException;
import java.time.Duration;
import java.util.concurrent.atomic.AtomicBoolean;

@RestController
@RequestMapping("/api/voice/sessions")
public class VoiceSessionController {
    private final VoiceSessionUseCase voiceSessionUseCase;

    public VoiceSessionController(VoiceSessionUseCase voiceSessionUseCase) {
        this.voiceSessionUseCase = voiceSessionUseCase;
    }

    @PostMapping
    public VoiceSessionCreateResponse open(@RequestHeader("X-API-Key") String apiKey,
                                           @Valid @RequestBody VoiceSessionCreateRequest request) {
        VoiceSessionOpenCommand command = new VoiceSessionOpenCommand();
        command.setSessionId(request.getSessionId());
        command.setApiKey(apiKey);
        command.setFeedbackModel(request.getFeedbackModel());
        command.setFeedbackLanguage(request.getFeedbackLanguage());
        command.setSttModel(request.getSttModel());
        command.setTtsModel(request.getTtsModel());
        command.setTtsVoice(request.getTtsVoice());

        VoiceSessionCreateResponse response = new VoiceSessionCreateResponse();
        response.setVoiceSessionId(voiceSessionUseCase.open(command));
        return response;
    }

    @GetMapping(value = "/{voiceSessionId}/events", produces = MediaType.TEXT_EVENT_STREAM_VALUE)
    public SseEmitter events(@PathVariable("voiceSessionId") String voiceSessionId,
                             @RequestParam(name = "sinceEventId", required = false) Long sinceEventId) {
        SseEmitter emitter = new SseEmitter(Duration.ofMinutes(30).toMillis());
        AtomicBoolean open = new AtomicBoolean(true);
        VoiceEventSink sink = toSink(emitter, open);
        registerCallbacks(emitter, open, voiceSessionId, sink);
        voiceSessionUseCase.registerSink(voiceSessionId, sink, sinceEventId);
        return emitter;
    }

    @GetMapping("/{voiceSessionId}/recovery")
    public VoiceSessionRecoveryResponse recover(@PathVariable("voiceSessionId") String voiceSessionId,
                                                @RequestParam(name = "lastSeenEventId", required = false) Long lastSeenEventId) {
        VoiceSessionUseCase.RecoverySnapshot snapshot = voiceSessionUseCase.recover(voiceSessionId, lastSeenEventId);
        return toRecoveryResponse(snapshot);
    }

    @PostMapping("/{voiceSessionId}/audio-chunks")
    @ResponseStatus(HttpStatus.NO_CONTENT)
    public void appendAudio(@PathVariable("voiceSessionId") String voiceSessionId,
                            @Valid @RequestBody VoiceAudioChunkRequest request) {
        VoiceAudioChunkCommand command = new VoiceAudioChunkCommand();
        command.setVoiceSessionId(voiceSessionId);
        command.setPcm16Base64(request.getPcm16Base64());
        command.setSampleRate(request.getSampleRate());
        command.setSequence(request.getSequence());
        voiceSessionUseCase.appendAudio(command);
    }

    @PostMapping("/{voiceSessionId}/stop")
    @ResponseStatus(HttpStatus.NO_CONTENT)
    public void stop(@PathVariable("voiceSessionId") String voiceSessionId,
                     @RequestBody(required = false) VoiceSessionStopRequest request) {
        VoiceSessionStopCommand command = new VoiceSessionStopCommand();
        command.setVoiceSessionId(voiceSessionId);
        command.setForced(request == null || request.getForced() == null || request.getForced());
        command.setReason(request == null ? "user_stop" : request.getReason());
        voiceSessionUseCase.stop(command);
    }

    private VoiceEventSink toSink(SseEmitter emitter, AtomicBoolean open) {
        return (event, data) -> {
            if (!open.get()) {
                return false;
            }
            try {
                emitter.send(SseEmitter.event().name(event).data(data));
                return true;
            } catch (IOException e) {
                emitter.complete();
                return false;
            }
        };
    }

    private void registerCallbacks(SseEmitter emitter,
                                   AtomicBoolean open,
                                   String voiceSessionId,
                                   VoiceEventSink sink) {
        emitter.onCompletion(() -> closeSink(open, voiceSessionId, sink));
        emitter.onTimeout(() -> closeSink(open, voiceSessionId, sink));
    }

    private void closeSink(AtomicBoolean open,
                           String voiceSessionId,
                           VoiceEventSink sink) {
        open.set(false);
        voiceSessionUseCase.unregisterSink(voiceSessionId, sink);
    }

    private VoiceSessionRecoveryResponse toRecoveryResponse(VoiceSessionUseCase.RecoverySnapshot snapshot) {
        VoiceSessionRecoveryResponse response = new VoiceSessionRecoveryResponse();
        response.setSessionId(snapshot.sessionId());
        response.setVoiceSessionId(snapshot.voiceSessionId());
        response.setActive(snapshot.active());
        response.setStopped(snapshot.stopped());
        response.setStopReason(snapshot.stopReason());
        response.setCurrentTurnId(snapshot.currentTurnId());
        response.setCurrentQuestion(toRecoveryQuestion(snapshot.currentQuestion()));
        response.setTurnProcessing(snapshot.turnProcessing());
        response.setHasBufferedAudio(snapshot.hasBufferedAudio());
        response.setLastAcceptedChunkSequence(snapshot.lastAcceptedChunkSequence());
        response.setLatestEventId(snapshot.latestEventId());
        response.setReplayFromEventId(snapshot.replayFromEventId());
        response.setGapDetected(snapshot.gapDetected());
        return response;
    }

    private VoiceSessionRecoveryResponse.RecoveryQuestion toRecoveryQuestion(VoiceSessionUseCase.RecoveryQuestion question) {
        if (question == null) {
            return null;
        }
        VoiceSessionRecoveryResponse.RecoveryQuestion response = new VoiceSessionRecoveryResponse.RecoveryQuestion();
        response.setId(question.id());
        response.setText(question.text());
        response.setGroup(question.group());
        response.setGroupId(question.groupId());
        response.setQuestionType(question.questionType());
        return response;
    }
}
