package me.go_gradually.omypic.application.stt.port;

import me.go_gradually.omypic.application.stt.model.VadSettings;

public interface SttGateway {
    String transcribe(byte[] fileBytes, String model, String apiKey, boolean translate, VadSettings vadSettings) throws Exception;
}
