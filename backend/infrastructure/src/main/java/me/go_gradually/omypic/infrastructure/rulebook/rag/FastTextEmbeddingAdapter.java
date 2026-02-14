package me.go_gradually.omypic.infrastructure.rulebook.rag;

import me.go_gradually.omypic.application.rulebook.port.EmbeddingPort;
import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.time.Duration;
import java.util.Collections;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;
import java.util.logging.Logger;
import java.util.zip.GZIPInputStream;

@Component
@ConditionalOnProperty(prefix = "omypic.rag", name = "provider", havingValue = "fasttext", matchIfMissing = true)
public class FastTextEmbeddingAdapter implements EmbeddingPort {
    private static final Logger log = Logger.getLogger(FastTextEmbeddingAdapter.class.getName());
    private static final String PROVIDER_NAME = "fasttext";
    private static final int EMBED_CACHE_MAX_ENTRIES = 1000;

    private final AppProperties.Rag rag;
    private final DataDirProvider dataDirProvider;
    private final LocalHashEmbeddingAdapter hashFallback;
    private final Map<String, float[]> embedCache;
    private final Object initLock = new Object();

    private volatile Map<String, float[]> vectors;
    private volatile RuntimeException initFailure;

    public FastTextEmbeddingAdapter(AppProperties properties, DataDirProvider dataDirProvider) {
        this.rag = properties.getRag();
        this.dataDirProvider = dataDirProvider;
        this.hashFallback = new LocalHashEmbeddingAdapter(properties);
        this.embedCache = createEmbedCache();
    }

    @Override
    public float[] embed(String text) {
        if (text == null || text.isBlank()) {
            return new float[dimension()];
        }
        String cacheKey = normalizeCacheKey(text);
        float[] cached = embedCache.get(cacheKey);
        if (cached != null) {
            return cached.clone();
        }
        Map<String, float[]> loaded = ensureVectors();
        float[] computed;
        if (loaded == null) {
            computed = hashFallback.embed(text);
            embedCache.put(cacheKey, computed.clone());
            return computed;
        }
        computed = embedFromModelOrFallback(text, loaded);
        embedCache.put(cacheKey, computed.clone());
        return computed;
    }

    private String normalizeCacheKey(String text) {
        return text.trim().toLowerCase(Locale.ROOT);
    }

    private Map<String, float[]> createEmbedCache() {
        return Collections.synchronizedMap(new LinkedHashMap<>(EMBED_CACHE_MAX_ENTRIES, 0.75f, true) {
            @Override
            protected boolean removeEldestEntry(Map.Entry<String, float[]> eldest) {
                return size() > EMBED_CACHE_MAX_ENTRIES;
            }
        });
    }

    private float[] embedFromModelOrFallback(String text, Map<String, float[]> loaded) {
        float[] sum = new float[dimension()];
        int hits = accumulateHits(text, loaded, sum);
        if (hits > 0) {
            normalize(sum);
            return sum;
        }
        if (rag.isAllowHashFallback()) {
            return hashFallback.embed(text);
        }
        return sum;
    }

    private int accumulateHits(String text, Map<String, float[]> loaded, float[] sum) {
        int hits = 0;
        for (String token : tokenize(text)) {
            float[] vector = loaded.get(token);
            if (vector == null) {
                continue;
            }
            add(sum, vector);
            hits++;
        }
        return hits;
    }

    private String[] tokenize(String text) {
        return text.toLowerCase(Locale.ROOT)
                .replaceAll("[^\\p{L}\\p{Nd}]+", " ")
                .trim()
                .split("\\s+");
    }

    private void add(float[] target, float[] source) {
        for (int i = 0; i < target.length; i++) {
            target[i] += source[i];
        }
    }

    private Map<String, float[]> ensureVectors() {
        if (vectors != null) {
            return vectors;
        }
        if (initFailure != null) {
            return handleInitializationFailure(initFailure);
        }
        synchronized (initLock) {
            if (vectors != null) {
                return vectors;
            }
            if (initFailure != null) {
                return handleInitializationFailure(initFailure);
            }
            try {
                Path modelPath = ensureModelFile();
                vectors = loadVectors(modelPath);
                log.info(() -> "RAG fasttext model loaded path=" + modelPath + " vocab=" + vectors.size());
                return vectors;
            } catch (RuntimeException e) {
                initFailure = e;
                return handleInitializationFailure(e);
            }
        }
    }

    private Map<String, float[]> handleInitializationFailure(RuntimeException failure) {
        if (rag.isAllowHashFallback()) {
            log.warning("FastText model unavailable; using hash fallback. reason=" + failure.getMessage());
            return null;
        }
        throw failure;
    }

    private Path ensureModelFile() {
        Path modelPath = resolveModelPath();
        ensureParentExists(modelPath);
        if (Files.exists(modelPath)) {
            verifyModelSha256(modelPath);
            return modelPath;
        }
        downloadModelWithRetry(modelPath);
        verifyModelSha256(modelPath);
        return modelPath;
    }

    private Path resolveModelPath() {
        if (rag.getModelPath() != null && !rag.getModelPath().isBlank()) {
            return Path.of(rag.getModelPath().trim());
        }
        String version = (rag.getModelVersion() == null || rag.getModelVersion().isBlank())
                ? "fasttext.vec.gz"
                : rag.getModelVersion().trim();
        return Path.of(dataDirProvider.getDataDir(), "models", version);
    }

    private void ensureParentExists(Path modelPath) {
        try {
            Path parent = modelPath.getParent();
            if (parent != null) {
                Files.createDirectories(parent);
            }
        } catch (IOException e) {
            throw new IllegalStateException("Failed to prepare model directory: " + modelPath, e);
        }
    }

    private void downloadModelWithRetry(Path modelPath) {
        String url = rag.getDownloadUrl();
        if (url == null || url.isBlank()) {
            throw new IllegalStateException("FastText model is missing and omypic.rag.download-url is not configured");
        }
        int attempts = Math.max(1, rag.getDownloadRetryMax() + 1);
        RuntimeException lastFailure = null;
        for (int attempt = 1; attempt <= attempts; attempt++) {
            try {
                downloadModel(modelPath, url.trim());
                return;
            } catch (RuntimeException e) {
                lastFailure = e;
                if (attempt == attempts) {
                    break;
                }
                sleepBackoff(attempt);
            }
        }
        throw new IllegalStateException("Failed to download FastText model after retries", lastFailure);
    }

    private void downloadModel(Path modelPath, String url) {
        Path tempPath = modelPath.resolveSibling(modelPath.getFileName() + ".part");
        HttpClient client = HttpClient.newBuilder()
                .connectTimeout(timeout())
                .build();
        HttpRequest request = HttpRequest.newBuilder(URI.create(url))
                .timeout(timeout())
                .GET()
                .build();
        try {
            HttpResponse<InputStream> response = client.send(request, HttpResponse.BodyHandlers.ofInputStream());
            if (response.statusCode() < 200 || response.statusCode() >= 300) {
                throw new IllegalStateException("Model download failed with status " + response.statusCode());
            }
            try (InputStream in = response.body()) {
                Files.copy(in, tempPath, StandardCopyOption.REPLACE_EXISTING);
            }
            moveAtomically(tempPath, modelPath);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("Model download interrupted", e);
        } catch (IOException e) {
            throw new IllegalStateException("Model download failed", e);
        } finally {
            deleteQuietly(tempPath);
        }
    }

    private Duration timeout() {
        return Duration.ofSeconds(Math.max(1, rag.getDownloadTimeoutSeconds()));
    }

    private void moveAtomically(Path source, Path target) throws IOException {
        try {
            Files.move(source, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException e) {
            Files.move(source, target, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private void deleteQuietly(Path path) {
        try {
            Files.deleteIfExists(path);
        } catch (IOException ignored) {
        }
    }

    private void sleepBackoff(int attempt) {
        long sleepMs = Math.min(5000L, 500L * (1L << Math.max(0, attempt - 1)));
        try {
            Thread.sleep(sleepMs);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("Interrupted while retrying model download", e);
        }
    }

    private void verifyModelSha256(Path modelPath) {
        String expected = rag.getModelSha256();
        if (expected == null || expected.isBlank()) {
            throw new IllegalStateException("omypic.rag.model-sha256 is required for fasttext provider");
        }
        String actual = sha256(modelPath);
        if (!expected.trim().equalsIgnoreCase(actual)) {
            throw new IllegalStateException("FastText model sha256 mismatch for " + modelPath);
        }
    }

    private String sha256(Path path) {
        MessageDigest digest = sha256Digest();
        byte[] buffer = new byte[8192];
        try (InputStream in = Files.newInputStream(path)) {
            int read;
            while ((read = in.read(buffer)) >= 0) {
                digest.update(buffer, 0, read);
            }
        } catch (IOException e) {
            throw new IllegalStateException("Failed to read model file for checksum: " + path, e);
        }
        return toHex(digest.digest());
    }

    private MessageDigest sha256Digest() {
        try {
            return MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException("SHA-256 algorithm is unavailable", e);
        }
    }

    private String toHex(byte[] hash) {
        StringBuilder builder = new StringBuilder(hash.length * 2);
        for (byte b : hash) {
            builder.append(String.format("%02x", b));
        }
        return builder.toString();
    }

    private Map<String, float[]> loadVectors(Path modelPath) {
        Map<String, float[]> loaded = new HashMap<>();
        int maxVocab = Math.max(1, rag.getModelMaxVocab());
        try (BufferedReader reader = openReader(modelPath)) {
            String firstLine = reader.readLine();
            if (firstLine == null) {
                throw new IllegalStateException("FastText model is empty: " + modelPath);
            }
            Integer headerDim = parseHeaderDimension(firstLine);
            validateDimension(headerDim);
            if (headerDim == null) {
                parseVectorLine(firstLine, loaded);
            }
            readRemainingVectors(reader, loaded, maxVocab);
        } catch (IOException e) {
            throw new IllegalStateException("Failed to load FastText vectors from " + modelPath, e);
        }
        if (loaded.isEmpty()) {
            throw new IllegalStateException("No vectors were loaded from model: " + modelPath);
        }
        return loaded;
    }

    private BufferedReader openReader(Path modelPath) throws IOException {
        InputStream input = Files.newInputStream(modelPath);
        if (modelPath.getFileName().toString().endsWith(".gz")) {
            input = new GZIPInputStream(input);
        }
        return new BufferedReader(new InputStreamReader(input, StandardCharsets.UTF_8));
    }

    private Integer parseHeaderDimension(String line) {
        String[] parts = line.trim().split("\\s+");
        if (parts.length != 2) {
            return null;
        }
        if (!isInteger(parts[0]) || !isInteger(parts[1])) {
            return null;
        }
        return Integer.parseInt(parts[1]);
    }

    private boolean isInteger(String text) {
        if (text == null || text.isEmpty()) {
            return false;
        }
        for (int i = 0; i < text.length(); i++) {
            if (!Character.isDigit(text.charAt(i))) {
                return false;
            }
        }
        return true;
    }

    private void validateDimension(Integer headerDim) {
        if (headerDim == null) {
            return;
        }
        if (headerDim != dimension()) {
            throw new IllegalStateException("Embedding dimension mismatch. expected=" + dimension() + " actual=" + headerDim);
        }
    }

    private void readRemainingVectors(BufferedReader reader, Map<String, float[]> loaded, int maxVocab) throws IOException {
        String line;
        while ((line = reader.readLine()) != null && loaded.size() < maxVocab) {
            parseVectorLine(line, loaded);
        }
    }

    private void parseVectorLine(String line, Map<String, float[]> loaded) {
        String trimmed = line == null ? "" : line.trim();
        if (trimmed.isEmpty()) {
            return;
        }
        String[] parts = trimmed.split("\\s+");
        if (parts.length < dimension() + 1) {
            return;
        }
        float[] vector = new float[dimension()];
        for (int i = 0; i < dimension(); i++) {
            vector[i] = parseFloat(parts[i + 1]);
        }
        normalize(vector);
        loaded.putIfAbsent(parts[0], vector);
    }

    private float parseFloat(String value) {
        try {
            return Float.parseFloat(value);
        } catch (NumberFormatException e) {
            return 0f;
        }
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

    @Override
    public int dimension() {
        return rag.getEmbeddingDim();
    }

    @Override
    public String provider() {
        return PROVIDER_NAME;
    }

    @Override
    public String modelVersion() {
        return rag.getModelVersion() == null || rag.getModelVersion().isBlank()
                ? "unknown-model"
                : rag.getModelVersion().trim();
    }
}
