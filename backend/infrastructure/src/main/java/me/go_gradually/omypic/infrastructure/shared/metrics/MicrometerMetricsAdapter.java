package me.go_gradually.omypic.infrastructure.shared.metrics;

import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.Timer;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import org.springframework.stereotype.Component;

import java.time.Duration;

@Component
public class MicrometerMetricsAdapter implements MetricsPort {
    private final MeterRegistry meterRegistry;

    public MicrometerMetricsAdapter(MeterRegistry meterRegistry) {
        this.meterRegistry = meterRegistry;
    }

    @Override
    public void recordSttLatency(Duration duration) {
        record("stt.latency", duration);
    }

    @Override
    public void recordFeedbackLatency(Duration duration) {
        record("feedback.latency", duration);
    }

    @Override
    public void recordTtsLatency(Duration duration) {
        record("tts.latency", duration);
    }

    @Override
    public void recordQuestionNextLatency(Duration duration) {
        record("question.next.latency", duration);
    }

    @Override
    public void recordVoiceTurnLatency(Duration duration) {
        record("voice.turn.latency", duration);
    }

    @Override
    public void recordRulebookUploadLatency(Duration duration) {
        record("rulebook.upload.latency", duration);
    }

    @Override
    public void incrementSttRequest() {
        meterRegistry.counter("stt.requests").increment();
    }

    @Override
    public void incrementSttError() {
        meterRegistry.counter("stt.errors").increment();
    }

    @Override
    public void incrementFeedbackError() {
        meterRegistry.counter("feedback.errors").increment();
    }

    @Override
    public void incrementFeedbackSchemaFallback() {
        meterRegistry.counter("feedback.schema_fallbacks").increment();
    }

    @Override
    public void incrementTtsError() {
        meterRegistry.counter("tts.errors").increment();
    }

    @Override
    public void incrementRecommendationDuplicateDetected() {
        meterRegistry.counter("feedback.recommendation.duplicate_detected").increment();
    }

    @Override
    public void incrementRecommendationRepairAttempt() {
        meterRegistry.counter("feedback.recommendation.repair_attempt").increment();
    }

    @Override
    public void incrementRecommendationRepairSuccess() {
        meterRegistry.counter("feedback.recommendation.repair_success").increment();
    }

    @Override
    public void incrementRecommendationMinimalFallback() {
        meterRegistry.counter("feedback.recommendation.minimal_fallback").increment();
    }

    private void record(String name, Duration duration) {
        Timer.builder(name)
                .publishPercentiles(0.5, 0.95, 0.99)
                .register(meterRegistry)
                .record(duration);
    }
}
