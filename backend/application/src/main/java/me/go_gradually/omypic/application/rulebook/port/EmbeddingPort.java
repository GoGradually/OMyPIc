package me.go_gradually.omypic.application.rulebook.port;

public interface EmbeddingPort {
    float[] embed(String text);

    int dimension();

    default String provider() {
        return "unknown";
    }

    default String modelVersion() {
        return "unknown";
    }
}
