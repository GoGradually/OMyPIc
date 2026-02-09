package me.go_gradually.omypic.infrastructure.stt.job;

import me.go_gradually.omypic.application.stt.model.SttJob;
import me.go_gradually.omypic.application.stt.port.SttJobStorePort;
import org.springframework.stereotype.Component;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

@Component
public class InMemorySttJobStore implements SttJobStorePort {
    private final Map<String, SttJob> jobs = new ConcurrentHashMap<>();

    @Override
    public SttJob create(String jobId, String sessionId) {
        SttJob job = new SttJob(jobId, sessionId);
        jobs.put(jobId, job);
        return job;
    }

    @Override
    public SttJob get(String jobId) {
        return jobs.get(jobId);
    }
}
