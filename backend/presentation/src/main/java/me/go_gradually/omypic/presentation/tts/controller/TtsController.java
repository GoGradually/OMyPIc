package me.go_gradually.omypic.presentation.tts.controller;

import jakarta.servlet.http.HttpServletResponse;
import jakarta.validation.Valid;
import me.go_gradually.omypic.application.tts.model.AudioSink;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.usecase.TtsUseCase;
import me.go_gradually.omypic.presentation.tts.dto.TtsRequest;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.server.ResponseStatusException;

@RestController
@RequestMapping("/api/tts")
public class TtsController {
    private final TtsUseCase ttsUseCase;
    private final boolean restDisabled;

    public TtsController(TtsUseCase ttsUseCase,
                         @Value("${omypic.realtime.rest-disabled:true}") boolean restDisabled) {
        this.ttsUseCase = ttsUseCase;
        this.restDisabled = restDisabled;
    }

    @PostMapping("/stream")
    public void stream(@RequestHeader("X-API-Key") String apiKey,
                       @Valid @RequestBody TtsRequest request,
                       HttpServletResponse response) throws Exception {
        assertLegacyRouteEnabled();
        response.setContentType("audio/mpeg");
        response.setStatus(200);
        AudioSink sink = bytes -> {
            response.getOutputStream().write(bytes);
            response.flushBuffer();
        };
        ttsUseCase.stream(apiKey, new TtsCommand(request.getText(), request.getVoice()), sink);
    }

    private void assertLegacyRouteEnabled() {
        if (restDisabled) {
            throw new ResponseStatusException(HttpStatus.GONE, "Use websocket /api/realtime/voice");
        }
    }
}
