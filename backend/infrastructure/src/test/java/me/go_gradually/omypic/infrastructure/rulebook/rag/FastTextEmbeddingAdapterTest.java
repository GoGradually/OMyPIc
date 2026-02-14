package me.go_gradually.omypic.infrastructure.rulebook.rag;

import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class FastTextEmbeddingAdapterTest {

    @TempDir
    Path tempDir;

    @Test
    void embed_loadsLocalModelAndNormalizesVector() throws IOException {
        byte[] modelBytes = (
                "3 3\n"
                        + "hello 1 0 0\n"
                        + "world 0 1 0\n"
                        + "alpha 0 0 1\n"
        ).getBytes(StandardCharsets.UTF_8);
        Path modelPath = tempDir.resolve("ko.vec");
        Files.write(modelPath, modelBytes);

        AppProperties properties = properties(modelPath, sha256(modelBytes));
        FastTextEmbeddingAdapter adapter = new FastTextEmbeddingAdapter(properties, () -> tempDir.toString());

        float[] vector = adapter.embed("hello world");

        assertEquals(3, vector.length);
        assertTrue(vector[0] > 0f);
        assertTrue(vector[1] > 0f);
        assertEquals(1.0, l2Norm(vector), 1e-6);
    }

    @Test
    void embed_downloadsModelWhenMissing() throws Exception {
        byte[] modelBytes = (
                "2 3\n"
                        + "hello 1 0 0\n"
                        + "world 0 1 0\n"
        ).getBytes(StandardCharsets.UTF_8);
        Path modelPath = tempDir.resolve("downloaded.vec");

        try (MockWebServer server = new MockWebServer()) {
            server.enqueue(new MockResponse().setResponseCode(200).setBody(new String(modelBytes, StandardCharsets.UTF_8)));
            server.start();

            AppProperties properties = properties(modelPath, sha256(modelBytes));
            properties.getRag().setDownloadUrl(server.url("/ko.vec").toString());
            FastTextEmbeddingAdapter adapter = new FastTextEmbeddingAdapter(properties, () -> tempDir.toString());

            float[] vector = adapter.embed("hello");

            assertTrue(Files.exists(modelPath));
            assertEquals(3, vector.length);
            assertTrue(vector[0] > 0f);
        }
    }

    @Test
    void embed_usesHashFallback_whenModelInitFailsAndFallbackEnabled() {
        Path missingModelPath = tempDir.resolve("missing.vec");
        AppProperties properties = properties(missingModelPath, "deadbeef");
        properties.getRag().setAllowHashFallback(true);
        properties.getRag().setDownloadUrl("");

        FastTextEmbeddingAdapter adapter = new FastTextEmbeddingAdapter(properties, () -> tempDir.toString());

        float[] vector = adapter.embed("fallback token");

        assertEquals(3, vector.length);
        assertTrue(l2Norm(vector) > 0.0);
    }

    private AppProperties properties(Path modelPath, String sha256) {
        AppProperties properties = new AppProperties();
        properties.setDataDir(tempDir.toString());
        properties.getRag().setProvider("fasttext");
        properties.getRag().setEmbeddingDim(3);
        properties.getRag().setModelPath(modelPath.toString());
        properties.getRag().setModelVersion(modelPath.getFileName().toString());
        properties.getRag().setModelSha256(sha256);
        properties.getRag().setModelMaxVocab(100);
        properties.getRag().setDownloadRetryMax(0);
        properties.getRag().setDownloadTimeoutSeconds(3);
        return properties;
    }

    private String sha256(byte[] bytes) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            byte[] hash = digest.digest(bytes);
            StringBuilder builder = new StringBuilder(hash.length * 2);
            for (byte b : hash) {
                builder.append(String.format("%02x", b));
            }
            return builder.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException("SHA-256 unavailable", e);
        }
    }

    private double l2Norm(float[] vector) {
        double sum = 0.0;
        for (float value : vector) {
            sum += value * value;
        }
        return Math.sqrt(sum);
    }
}
