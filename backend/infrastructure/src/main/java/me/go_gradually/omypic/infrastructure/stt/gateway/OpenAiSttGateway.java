package me.go_gradually.omypic.infrastructure.stt.gateway;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.stt.model.VadSettings;
import me.go_gradually.omypic.application.stt.port.SttGateway;
import org.springframework.http.MediaType;
import org.springframework.http.client.MultipartBodyBuilder;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.BodyInserters;
import org.springframework.web.reactive.function.client.WebClient;

@Component
public class OpenAiSttGateway implements SttGateway {
    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();

    public OpenAiSttGateway(WebClient webClient) {
        this.webClient = webClient;
    }

    @Override
    public String transcribe(byte[] fileBytes, String model, String apiKey, boolean translate, VadSettings vadSettings) throws Exception {
        String response = callOpenAi(fileBytes, model, apiKey, translate, vadSettings);
        JsonNode root = objectMapper.readTree(response);
        return root.path("text").asText("");
    }

    private String callOpenAi(byte[] fileBytes, String model, String apiKey, boolean translate, VadSettings vadSettings) {
        MultipartBodyBuilder builder = new MultipartBodyBuilder();
        builder.part("file", fileBytes)
                .header("Content-Disposition", "form-data; name=file; filename=audio.webm")
                .contentType(MediaType.APPLICATION_OCTET_STREAM);
        builder.part("model", model);
        builder.part("response_format", "json");
        builder.part("prefix_padding_ms", String.valueOf(vadSettings.prefixPaddingMs()));
        builder.part("silence_duration_ms", String.valueOf(vadSettings.silenceDurationMs()));
        builder.part("threshold", String.valueOf(vadSettings.threshold()));

        String endpoint = translate ? "https://api.openai.com/v1/audio/translations" : "https://api.openai.com/v1/audio/transcriptions";
        return webClient.post()
                .uri(endpoint)
                .header("Authorization", "Bearer " + apiKey)
                .contentType(MediaType.MULTIPART_FORM_DATA)
                .body(BodyInserters.fromMultipartData(builder.build()))
                .retrieve()
                .bodyToMono(String.class)
                .block();
    }
}
