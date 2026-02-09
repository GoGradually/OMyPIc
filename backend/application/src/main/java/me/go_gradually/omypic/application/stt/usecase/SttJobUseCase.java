package me.go_gradually.omypic.application.stt.usecase;

import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.model.SttEventSink;
import me.go_gradually.omypic.application.stt.model.SttJob;
import me.go_gradually.omypic.application.stt.port.SttJobStorePort;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.shared.util.TextUtils;

import java.util.List;
import java.util.UUID;

public class SttJobUseCase {
    private final SttUseCase sttUseCase;
    private final SessionStorePort sessionStore;
    private final SttJobStorePort jobStore;
    private final AsyncExecutor asyncExecutor;

    public SttJobUseCase(SttUseCase sttUseCase,
                         SessionStorePort sessionStore,
                         SttJobStorePort jobStore,
                         AsyncExecutor asyncExecutor) {
        this.sttUseCase = sttUseCase;
        this.sessionStore = sessionStore;
        this.jobStore = jobStore;
        this.asyncExecutor = asyncExecutor;
    }

    public String createJob(SttCommand command) {
        String jobId = UUID.randomUUID().toString();
        SttJob job = jobStore.create(jobId, command.getSessionId());
        asyncExecutor.execute(() -> process(job, command));
        return jobId;
    }

    public void registerSink(String jobId, SttEventSink sink) {
        SttJob job = jobStore.get(jobId);
        if (job == null) {
            throw new IllegalArgumentException("Unknown job");
        }
        job.getSinks().add(sink);
        if (job.isDone() && job.getText() != null) {
            sink.send("final", job.getText());
        }
    }

    public void unregisterSink(String jobId, SttEventSink sink) {
        SttJob job = jobStore.get(jobId);
        if (job == null) {
            return;
        }
        job.getSinks().remove(sink);
    }

    private void process(SttJob job, SttCommand command) {
        try {
            String text = sttUseCase.transcribe(command);
            job.setText(text);
            job.setDone(true);
            if (job.getSessionId() != null && !job.getSessionId().isBlank()) {
                sessionStore.getOrCreate(SessionId.of(job.getSessionId())).appendSegment(text);
            }
            List<String> parts = TextUtils.splitChunks(text, Math.max(1, text.length() / 3 + 1));
            for (String part : parts) {
                sendEvent(job, "partial", part);
            }
            sendEvent(job, "final", text);
        } catch (Exception e) {
            job.setError(e.getMessage());
            job.setDone(true);
            sendEvent(job, "error", job.getError());
        }
    }

    private void sendEvent(SttJob job, String event, String data) {
        for (SttEventSink sink : job.getSinks()) {
            boolean ok = sink.send(event, data);
            if (!ok) {
                job.getSinks().remove(sink);
            }
        }
    }
}
