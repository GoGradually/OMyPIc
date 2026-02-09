package me.go_gradually.omypic.presentation.stt.controller;

import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.model.SttEventSink;
import me.go_gradually.omypic.application.stt.usecase.SttJobUseCase;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.presentation.stt.dto.SttUploadResponse;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.multipart.MultipartFile;
import org.springframework.web.server.ResponseStatusException;
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
    private final boolean restDisabled;

    public SttController(SttUseCase sttUseCase,
                         SttJobUseCase jobUseCase,
                         SessionUseCase sessionUseCase,
                         @Value("${omypic.realtime.rest-disabled:true}") boolean restDisabled) {
        this.sttUseCase = sttUseCase;
        this.jobUseCase = jobUseCase;
        this.sessionUseCase = sessionUseCase;
        this.restDisabled = restDisabled;
    }

    @PostMapping(value = "/upload", consumes = MediaType.MULTIPART_FORM_DATA_VALUE)
    public SttUploadResponse upload(@RequestHeader("X-API-Key") String apiKey,
                                    @RequestParam("file") MultipartFile file,
                                    @RequestParam(value = "model", defaultValue = "gpt-4o-mini-transcribe") String model,
                                    @RequestParam(value = "stream", defaultValue = "false") boolean stream,
                                    @RequestParam(value = "translate", defaultValue = "false") boolean translate,
                                    @RequestParam(value = "sessionId", required = false) String sessionId) {
        assertLegacyRouteEnabled();
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
        assertLegacyRouteEnabled();
        SseEmitter emitter = new SseEmitter(Duration.ofMinutes(5).toMillis());
        AtomicBoolean open = new AtomicBoolean(true);
        SttEventSink sink = (event, data) -> {
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
        emitter.onCompletion(() -> {
            open.set(false);
            jobUseCase.unregisterSink(jobId, sink);
        });
        emitter.onTimeout(() -> {
            open.set(false);
            jobUseCase.unregisterSink(jobId, sink);
        });
        jobUseCase.registerSink(jobId, sink);
        return emitter;
    }

    private void assertLegacyRouteEnabled() {
        if (restDisabled) {
            throw new ResponseStatusException(HttpStatus.GONE, "Use websocket /api/realtime/voice");
        }
    }
}
