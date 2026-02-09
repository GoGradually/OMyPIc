package me.go_gradually.omypic.infrastructure.tts.gateway;

import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.port.TtsGateway;
import org.springframework.core.io.buffer.DataBuffer;
import org.springframework.http.MediaType;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;
import reactor.core.publisher.Flux;

import java.util.Iterator;
import java.util.Map;

@Component
public class OpenAiTtsGateway implements TtsGateway {
    private static final String DEFAULT_ENDPOINT = "https://api.openai.com/v1/audio/speech";

    private final WebClient webClient;
    private final String endpoint;

    public OpenAiTtsGateway(WebClient webClient) {
        this(webClient, DEFAULT_ENDPOINT);
    }

    OpenAiTtsGateway(WebClient webClient, String endpoint) {
        this.webClient = webClient;
        this.endpoint = endpoint;
    }

    @Override
    public Iterable<byte[]> stream(String apiKey, TtsCommand request) {
        Map<String, Object> payload = Map.of(
                "model", "gpt-4o-mini-tts",
                "voice", request.getVoice(),
                "input", request.getText(),
                "format", "mp3",
                "stream", true
        );

        Flux<DataBuffer> data = webClient.post()
                .uri(endpoint)
                .header("Authorization", "Bearer " + apiKey)
                .contentType(MediaType.APPLICATION_JSON)
                .accept(MediaType.APPLICATION_OCTET_STREAM)
                .bodyValue(payload)
                .retrieve()
                .bodyToFlux(DataBuffer.class);

        return () -> toByteIterator(data);
    }

    private Iterator<byte[]> toByteIterator(Flux<DataBuffer> data) {
        return data.toStream()
                .map(buffer -> {
                    byte[] bytes = new byte[buffer.readableByteCount()];
                    buffer.read(bytes);
                    return bytes;
                })
                .iterator();
    }
}
