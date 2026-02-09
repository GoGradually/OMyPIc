package me.go_gradually.omypic.application.realtime.model;

public interface RealtimeAudioSession {
    void appendBase64Audio(String base64Audio);

    void commit();

    void cancelResponse();

    void close();
}
