package me.go_gradually.omypic.infrastructure.rulebook.rag;

import me.go_gradually.omypic.application.rulebook.port.EmbeddingPort;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.stereotype.Component;

import java.nio.charset.StandardCharsets;
import java.util.Locale;

@Component
public class LocalHashEmbeddingAdapter implements EmbeddingPort {
    private final int dimension;

    public LocalHashEmbeddingAdapter(AppProperties properties) {
        this.dimension = properties.getRag().getEmbeddingDim();
    }

    @Override
    public float[] embed(String text) {
        float[] vector = new float[dimension];
        if (text == null || text.isBlank()) {
            return vector;
        }
        for (String token : tokenize(text)) {
            addToken(vector, token);
        }
        normalize(vector);
        return vector;
    }

    private String[] tokenize(String text) {
        return text.toLowerCase(Locale.ROOT).split("\\s+");
    }

    private void addToken(float[] vector, String token) {
        if (token.isBlank()) {
            return;
        }
        int bucket = Math.floorMod(hash(token), dimension);
        vector[bucket] += 1.0f;
    }

    @Override
    public int dimension() {
        return dimension;
    }

    private int hash(String token) {
        byte[] bytes = token.getBytes(StandardCharsets.UTF_8);
        int h = 0;
        for (byte b : bytes) {
            h = 31 * h + b;
        }
        return h;
    }

    private void normalize(float[] vector) {
        double sum = 0.0;
        for (float v : vector) {
            sum += v * v;
        }
        if (sum == 0.0) {
            return;
        }
        double norm = Math.sqrt(sum);
        for (int i = 0; i < vector.length; i++) {
            vector[i] = (float) (vector[i] / norm);
        }
    }
}
