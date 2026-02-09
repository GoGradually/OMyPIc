package me.go_gradually.omypic.application.stt.port;

import me.go_gradually.omypic.application.stt.model.SttJob;

public interface SttJobStorePort {
    SttJob create(String jobId, String sessionId);

    SttJob get(String jobId);
}
