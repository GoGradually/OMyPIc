package me.go_gradually.omypic.infrastructure.voice.gateway;

import me.go_gradually.omypic.application.voice.port.TtsGateway;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.ai.openai.api.OpenAiAudioApi;
import org.springframework.stereotype.Component;

@Component
public class OpenAiTtsGateway implements TtsGateway {
    private final String baseUrl;

    public OpenAiTtsGateway(AppProperties properties) {
        this.baseUrl = properties.getIntegrations().getOpenai().getBaseUrl();
    }

    @Override
    public byte[] synthesize(String apiKey, String model, String voice, String text) {
        if (text == null || text.isBlank()) {
            return new byte[0];
        }
        OpenAiAudioApi.SpeechRequest request = new OpenAiAudioApi.SpeechRequest(
                model,
                text,
                voice,
                OpenAiAudioApi.SpeechRequest.AudioResponseFormat.WAV,
                null
        );
        byte[] body = audioApi(apiKey).createSpeech(request).getBody();
        return body == null ? new byte[0] : body;
    }

    private OpenAiAudioApi audioApi(String apiKey) {
        return OpenAiAudioApi.builder()
                .baseUrl(baseUrl)
                .apiKey(apiKey)
                .build();
    }
}
