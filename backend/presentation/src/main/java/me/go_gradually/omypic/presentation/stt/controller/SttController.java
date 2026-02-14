package me.go_gradually.omypic.presentation.stt.controller;

import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.model.SttEventSink;
import me.go_gradually.omypic.application.stt.usecase.SttJobUseCase;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.presentation.stt.dto.SttUploadResponse;
import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.multipart.MultipartFile;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;

import java.io.IOException;
import java.time.Duration;
import java.util.concurrent.atomic.AtomicBoolean;

@RestController
@RequestMapping("/api/stt")
public class SttController {
    private final SttUseCase sttUseCase;
    private final SttJobUseCase jobUseCase;
    private final SessionUseCase sessionUseCase;

    public SttController(SttUseCase sttUseCase,
                         SttJobUseCase jobUseCase,
                         SessionUseCase sessionUseCase) {
        this.sttUseCase = sttUseCase;
        this.jobUseCase = jobUseCase;
        this.sessionUseCase = sessionUseCase;
    }

    @PostMapping(value = "/upload", consumes = MediaType.MULTIPART_FORM_DATA_VALUE)
    public SttUploadResponse upload(@RequestHeader("X-API-Key") String apiKey,
                                    @RequestParam("file") MultipartFile file,
                                    @RequestParam(value = "model", defaultValue = "gpt-4o-mini-transcribe") String model,
                                    @RequestParam(value = "stream", defaultValue = "false") boolean stream,
                                    @RequestParam(value = "translate", defaultValue = "false") boolean translate,
                                    @RequestParam(value = "sessionId", required = false) String sessionId) {
        SttCommand command = new SttCommand();
        try {
            command.setFileBytes(file.getBytes());
        } catch (IOException e) {
            throw new IllegalStateException("Failed to read upload", e);
        }
        command.setModel(model);
        command.setApiKey(apiKey);
        command.setTranslate(translate);
        command.setSessionId(sessionId);
        if (stream) {
            String jobId = jobUseCase.createJob(command);
            return SttUploadResponse.forJob(jobId);
        }
        String text = sttUseCase.transcribe(command);
        if (sessionId != null && !sessionId.isBlank()) {
            sessionUseCase.appendSegment(sessionId, text);
        }
        return SttUploadResponse.forText(text);
    }

    @GetMapping(value = "/stream", produces = MediaType.TEXT_EVENT_STREAM_VALUE)
    public SseEmitter stream(@RequestParam("jobId") String jobId) {
        SseEmitter emitter = new SseEmitter(Duration.ofMinutes(5).toMillis());
        AtomicBoolean open = new AtomicBoolean(true);
        SttEventSink sink = toEventSink(emitter, open);
        registerEmitterCallbacks(emitter, open, jobId, sink);
        jobUseCase.registerSink(jobId, sink);
        return emitter;
    }

    private SttEventSink toEventSink(SseEmitter emitter, AtomicBoolean open) {
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

    private void registerEmitterCallbacks(SseEmitter emitter,
                                          AtomicBoolean open,
                                          String jobId,
                                          SttEventSink sink) {
        emitter.onCompletion(() -> closeSink(open, jobId, sink));
        emitter.onTimeout(() -> closeSink(open, jobId, sink));
    }

    private void closeSink(AtomicBoolean open, String jobId, SttEventSink sink) {
        open.set(false);
        jobUseCase.unregisterSink(jobId, sink);
    }
}
