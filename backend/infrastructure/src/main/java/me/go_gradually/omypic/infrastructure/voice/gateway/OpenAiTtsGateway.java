package me.go_gradually.omypic.infrastructure.voice.gateway;

import me.go_gradually.omypic.application.voice.port.TtsGateway;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.http.MediaType;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;

import java.util.Map;

@Component
public class OpenAiTtsGateway implements TtsGateway {
    private final WebClient webClient;

    public OpenAiTtsGateway(@Qualifier("openAiWebClient") WebClient webClient) {
        this.webClient = webClient;
    }

    @Override
    public byte[] synthesize(String apiKey, String model, String voice, String text) {
        if (text == null || text.isBlank()) {
            return new byte[0];
        }
        return webClient.post()
                .uri("/v1/audio/speech")
                .header("Authorization", "Bearer " + apiKey)
                .contentType(MediaType.APPLICATION_JSON)
                .accept(MediaType.APPLICATION_OCTET_STREAM)
                .bodyValue(payload(model, voice, text))
                .retrieve()
                .bodyToMono(byte[].class)
                .block();
    }

    private Map<String, Object> payload(String model, String voice, String text) {
        return Map.of(
                "model", model,
                "voice", voice,
                "input", text,
                "response_format", "wav"
        );
    }
}
