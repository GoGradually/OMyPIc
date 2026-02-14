package me.go_gradually.omypic.application.voice.port;

public interface TtsGateway {
    byte[] synthesize(String apiKey, String model, String voice, String text) throws Exception;
}
