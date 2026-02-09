package me.go_gradually.omypic.application.stt.model;

import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

public class SttJob {
    private final String jobId;
    private final String sessionId;
    private final List<SttEventSink> sinks = new CopyOnWriteArrayList<>();
    private volatile String text;
    private volatile String error;
    private volatile boolean done;

    public SttJob(String jobId, String sessionId) {
        this.jobId = jobId;
        this.sessionId = sessionId;
    }

    public String getJobId() {
        return jobId;
    }

    public String getSessionId() {
        return sessionId;
    }

    public List<SttEventSink> getSinks() {
        return sinks;
    }

    public String getText() {
        return text;
    }

    public void setText(String text) {
        this.text = text;
    }

    public String getError() {
        return error;
    }

    public void setError(String error) {
        this.error = error;
    }

    public boolean isDone() {
        return done;
    }

    public void setDone(boolean done) {
        this.done = done;
    }
}
