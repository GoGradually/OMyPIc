package me.go_gradually.omypic.infrastructure.shared.metrics;

import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import org.junit.jupiter.api.Test;

import java.time.Duration;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

class MicrometerMetricsAdapterTest {

    @Test
    void recordsTimersAndErrorCounters() {
        SimpleMeterRegistry registry = new SimpleMeterRegistry();
        MicrometerMetricsAdapter adapter = new MicrometerMetricsAdapter(registry);

        adapter.recordSttLatency(Duration.ofMillis(10));
        adapter.recordFeedbackLatency(Duration.ofMillis(20));
        adapter.recordTtsLatency(Duration.ofMillis(30));
        adapter.recordQuestionNextLatency(Duration.ofMillis(40));
        adapter.recordVoiceTurnLatency(Duration.ofMillis(50));
        adapter.recordRulebookUploadLatency(Duration.ofMillis(60));

        adapter.incrementSttRequest();
        adapter.incrementSttError();
        adapter.incrementFeedbackError();
        adapter.incrementFeedbackSchemaFallback();
        adapter.incrementTtsError();
        adapter.incrementRecommendationDuplicateDetected();
        adapter.incrementRecommendationRepairAttempt();
        adapter.incrementRecommendationRepairSuccess();
        adapter.incrementRecommendationMinimalFallback();

        assertNotNull(registry.find("stt.latency").timer());
        assertEquals(1, registry.find("stt.latency").timer().count());
        assertEquals(1, registry.find("feedback.latency").timer().count());
        assertEquals(1, registry.find("tts.latency").timer().count());
        assertEquals(1, registry.find("question.next.latency").timer().count());
        assertEquals(1, registry.find("voice.turn.latency").timer().count());
        assertEquals(1, registry.find("rulebook.upload.latency").timer().count());

        assertEquals(1.0, registry.find("stt.requests").counter().count());
        assertEquals(1.0, registry.find("stt.errors").counter().count());
        assertEquals(1.0, registry.find("feedback.errors").counter().count());
        assertEquals(1.0, registry.find("feedback.schema_fallbacks").counter().count());
        assertEquals(1.0, registry.find("tts.errors").counter().count());
        assertEquals(1.0, registry.find("feedback.recommendation.duplicate_detected").counter().count());
        assertEquals(1.0, registry.find("feedback.recommendation.repair_attempt").counter().count());
        assertEquals(1.0, registry.find("feedback.recommendation.repair_success").counter().count());
        assertEquals(1.0, registry.find("feedback.recommendation.minimal_fallback").counter().count());
    }
}
