package me.go_gradually.omypic.application.shared.port;

import java.time.Duration;

public interface MetricsPort {
    void recordSttLatency(Duration duration);

    void recordFeedbackLatency(Duration duration);

    void recordTtsLatency(Duration duration);

    void recordQuestionNextLatency(Duration duration);

    void recordRealtimeTurnLatency(Duration duration);

    void recordRulebookUploadLatency(Duration duration);

    void incrementSttRequest();

    void incrementSttError();

    void incrementFeedbackError();

    void incrementFeedbackSchemaFallback();

    void incrementTtsError();
}
