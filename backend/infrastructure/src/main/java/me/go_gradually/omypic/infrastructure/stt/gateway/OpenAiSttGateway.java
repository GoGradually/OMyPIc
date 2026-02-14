package me.go_gradually.omypic.infrastructure.stt.gateway;

import me.go_gradually.omypic.application.stt.model.VadSettings;
import me.go_gradually.omypic.application.stt.port.SttGateway;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.ai.openai.api.OpenAiAudioApi;
import org.springframework.stereotype.Component;

import java.util.Map;

@Component
public class OpenAiSttGateway implements SttGateway {
    private final String baseUrl;

    public OpenAiSttGateway(AppProperties properties) {
        this.baseUrl = properties.getIntegrations().getOpenai().getBaseUrl();
    }

    @Override
    public String transcribe(byte[] fileBytes, String model, String apiKey, boolean translate, VadSettings vadSettings) throws Exception {
        OpenAiAudioApi audioApi = audioApi(apiKey);
        byte[] safeBytes = fileBytes == null ? new byte[0] : fileBytes;
        Map<?, ?> response;
        if (translate) {
            OpenAiAudioApi.TranslationRequest request = new OpenAiAudioApi.TranslationRequest(
                    safeBytes,
                    "audio.wav",
                    model,
                    null,
                    OpenAiAudioApi.TranscriptResponseFormat.JSON,
                    null
            );
            response = audioApi.createTranslation(request, Map.class).getBody();
        } else {
            OpenAiAudioApi.TranscriptionRequest request = new OpenAiAudioApi.TranscriptionRequest(
                    safeBytes,
                    "audio.wav",
                    model,
                    null,
                    null,
                    OpenAiAudioApi.TranscriptResponseFormat.JSON,
                    null,
                    null
            );
            response = audioApi.createTranscription(request, Map.class).getBody();
        }
        Object text = response == null ? null : response.get("text");
        return text == null ? "" : String.valueOf(text);
    }

    private OpenAiAudioApi audioApi(String apiKey) {
        return OpenAiAudioApi.builder()
                .baseUrl(baseUrl)
                .apiKey(apiKey)
                .build();
    }
}
