package me.go_gradually.omypic.application.realtime.model;

public interface RealtimeAudioEventListener {
    void onPartialTranscript(String delta);

    void onFinalTranscript(String text);

    void onAssistantAudioChunk(long turnId, String base64Audio);

    void onAssistantAudioCompleted(long turnId);

    void onAssistantAudioFailed(long turnId, String message);

    void onError(String message);
}
