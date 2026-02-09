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
        if (command.getFileBytes() == null || command.getFileBytes().length > sttPolicy.getMaxFileBytes()) {
            throw new IllegalArgumentException("File too large");
        }
        metrics.incrementSttRequest();
        int attempt = 0;
        Exception last = null;
        Instant start = Instant.now();
        while (attempt <= sttPolicy.retryMax()) {
            try {
                String text = sttGateway.transcribe(command.getFileBytes(), command.getModel(),
                        command.getApiKey(), command.isTranslate(), sttPolicy.getVadSettings());
                metrics.recordSttLatency(Duration.between(start, Instant.now()));
                return text;
            } catch (Exception e) {
                last = e;
                attempt++;
            }
        }
        metrics.incrementSttError();
        throw new IllegalStateException("STT failed", last);
    }
}
