package me.go_gradually.omypic.application.stt.usecase;

import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.policy.SttPolicy;
import me.go_gradually.omypic.application.stt.port.SttGateway;

import java.time.Duration;
import java.time.Instant;

public class SttUseCase {
    private final SttGateway sttGateway;
    private final SttPolicy sttPolicy;
    private final MetricsPort metrics;

    public SttUseCase(SttGateway sttGateway, SttPolicy sttPolicy, MetricsPort metrics) {
        this.sttGateway = sttGateway;
        this.sttPolicy = sttPolicy;
        this.metrics = metrics;
    }

    public String transcribe(SttCommand command) {
        validateFile(command);
        metrics.incrementSttRequest();
        Instant start = Instant.now();
        try {
            return transcribeWithRetry(command, start);
        } catch (Exception e) {
            metrics.incrementSttError();
            throw new IllegalStateException("STT failed", e);
        }
    }

    private void validateFile(SttCommand command) {
        if (command.getFileBytes() == null || command.getFileBytes().length > sttPolicy.getMaxFileBytes()) {
            throw new IllegalArgumentException("File too large");
        }
    }

    private String transcribeWithRetry(SttCommand command, Instant start) throws Exception {
        int attempt = 0;
        Exception lastError = new IllegalStateException("STT failed");
        while (attempt <= sttPolicy.retryMax()) {
            try {
                return transcribeOnce(command, start);
            } catch (Exception e) {
                lastError = e;
                attempt++;
            }
        }
        throw lastError;
    }

    private String transcribeOnce(SttCommand command, Instant start) throws Exception {
        String text = sttGateway.transcribe(command.getFileBytes(), command.getModel(),
                command.getApiKey(), command.isTranslate(), sttPolicy.getVadSettings());
        metrics.recordSttLatency(Duration.between(start, Instant.now()));
        return text;
    }
}
