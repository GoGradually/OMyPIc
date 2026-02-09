package me.go_gradually.omypic.presentation.tts.controller;

import jakarta.servlet.http.HttpServletResponse;
import jakarta.validation.Valid;
import me.go_gradually.omypic.application.tts.model.AudioSink;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.usecase.TtsUseCase;
import me.go_gradually.omypic.presentation.tts.dto.TtsRequest;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/tts")
public class TtsController {
    private final TtsUseCase ttsUseCase;

    public TtsController(TtsUseCase ttsUseCase) {
        this.ttsUseCase = ttsUseCase;
    }

    @PostMapping("/stream")
    public void stream(@RequestHeader("X-API-Key") String apiKey,
                       @Valid @RequestBody TtsRequest request,
                       HttpServletResponse response) throws Exception {
        response.setContentType("audio/mpeg");
        response.setStatus(200);
        AudioSink sink = bytes -> {
            response.getOutputStream().write(bytes);
            response.flushBuffer();
        };
        ttsUseCase.stream(apiKey, new TtsCommand(request.getText(), request.getVoice()), sink);
    }
}
