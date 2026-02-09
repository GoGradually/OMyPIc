package me.go_gradually.omypic.application.realtime.model;

public interface RealtimeAudioEventListener {
    void onPartialTranscript(String delta);

    void onFinalTranscript(String text);

    void onError(String message);
}
