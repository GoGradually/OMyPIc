package me.go_gradually.omypic.application.tts.model;

import java.io.IOException;

public interface AudioSink {
    void write(byte[] bytes) throws IOException;
}
