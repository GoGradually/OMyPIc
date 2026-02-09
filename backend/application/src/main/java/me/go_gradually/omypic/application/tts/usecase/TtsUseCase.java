package me.go_gradually.omypic.application.tts.usecase;

import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.tts.model.AudioSink;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.port.TtsGateway;

import java.time.Duration;
import java.time.Instant;
import java.util.function.BooleanSupplier;

public class TtsUseCase {
    private final TtsGateway ttsGateway;
    private final MetricsPort metrics;

    public TtsUseCase(TtsGateway ttsGateway, MetricsPort metrics) {
        this.ttsGateway = ttsGateway;
        this.metrics = metrics;
    }

    public void stream(String apiKey, TtsCommand command, AudioSink sink) throws Exception {
        streamUntil(apiKey, command, sink, () -> false);
    }

    public void streamUntil(String apiKey, TtsCommand command, AudioSink sink, BooleanSupplier shouldStop) throws Exception {
        Instant start = Instant.now();
        try {
            for (byte[] bytes : ttsGateway.stream(apiKey, command)) {
                if (shouldStop.getAsBoolean()) {
                    metrics.recordTtsLatency(Duration.between(start, Instant.now()));
                    return;
                }
                sink.write(bytes);
            }
            metrics.recordTtsLatency(Duration.between(start, Instant.now()));
        } catch (Exception e) {
            metrics.recordTtsLatency(Duration.between(start, Instant.now()));
            metrics.incrementTtsError();
            throw e;
        }
    }
}
