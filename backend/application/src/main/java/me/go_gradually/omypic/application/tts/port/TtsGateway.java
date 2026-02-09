package me.go_gradually.omypic.application.tts.port;

import me.go_gradually.omypic.application.tts.model.TtsCommand;

public interface TtsGateway {
    Iterable<byte[]> stream(String apiKey, TtsCommand command) throws Exception;
}
