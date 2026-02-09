package me.go_gradually.omypic.application.rulebook.port;

public interface EmbeddingPort {
    float[] embed(String text);

    int dimension();
}
