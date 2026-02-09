package me.go_gradually.omypic.infrastructure.rulebook.rag;

import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

class LocalHashEmbeddingAdapterTest {

    @Test
    void embed_returnsZeroVector_forNullOrBlankText() {
        LocalHashEmbeddingAdapter adapter = new LocalHashEmbeddingAdapter(properties(8));

        float[] nullVector = adapter.embed(null);
        float[] blankVector = adapter.embed("   ");

        assertEquals(8, nullVector.length);
        assertEquals(8, blankVector.length);
        assertEquals(0.0, l2Norm(nullVector), 1e-9);
        assertEquals(0.0, l2Norm(blankVector), 1e-9);
    }

    @Test
    void embed_normalizesNonEmptyVector() {
        LocalHashEmbeddingAdapter adapter = new LocalHashEmbeddingAdapter(properties(8));

        float[] vector = adapter.embed("Hello hello world");

        assertEquals(8, vector.length);
        assertEquals(1.0, l2Norm(vector), 1e-6);
    }

    @Test
    void dimension_returnsConfiguredDimension() {
        LocalHashEmbeddingAdapter adapter = new LocalHashEmbeddingAdapter(properties(16));

        assertEquals(16, adapter.dimension());
    }

    private AppProperties properties(int dim) {
        AppProperties properties = new AppProperties();
        properties.getRag().setEmbeddingDim(dim);
        return properties;
    }

    private double l2Norm(float[] vector) {
        double sum = 0.0;
        for (float v : vector) {
            sum += v * v;
        }
        return Math.sqrt(sum);
    }
}
