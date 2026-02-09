package me.go_gradually.omypic.application.stt.policy;

import me.go_gradually.omypic.application.stt.model.VadSettings;

public interface SttPolicy {
    long getMaxFileBytes();

    int getRetryMax();

    VadSettings getVadSettings();
}
