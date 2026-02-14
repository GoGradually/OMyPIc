package me.go_gradually.omypic.application.voice.model;

@FunctionalInterface
public interface VoiceEventSink {
    boolean send(String event, Object payload);
}
