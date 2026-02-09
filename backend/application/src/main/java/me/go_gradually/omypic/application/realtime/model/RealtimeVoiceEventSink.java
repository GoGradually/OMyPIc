package me.go_gradually.omypic.application.realtime.model;

@FunctionalInterface
public interface RealtimeVoiceEventSink {
    boolean send(String event, Object data);
}
