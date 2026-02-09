package me.go_gradually.omypic.application.realtime.model;

public interface RealtimeVoiceSession {
    void appendAudio(String base64Audio);

    void commitAudio();

    void cancelResponse();

    void update(RealtimeSessionUpdateCommand command);

    void close();
}
