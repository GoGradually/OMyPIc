package me.go_gradually.omypic.application.feedback.port;

public interface LlmClient {
    String provider();

    String generate(String apiKey, String model, String systemPrompt, String userPrompt) throws Exception;
}
