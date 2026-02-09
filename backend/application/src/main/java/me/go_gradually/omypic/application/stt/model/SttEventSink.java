package me.go_gradually.omypic.application.stt.model;

public interface SttEventSink {
    boolean send(String event, String data);
}
