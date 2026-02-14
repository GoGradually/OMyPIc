package me.go_gradually.omypic.application.realtime.policy;

public interface RealtimePolicy {
    String realtimeConversationModel();

    String realtimeSttModel();

    int realtimeVadPrefixPaddingMs();

    int realtimeVadSilenceDurationMs();

    double realtimeVadThreshold();

    String realtimeFeedbackModel();

    String realtimeFeedbackLanguage();

    String realtimeTtsVoice();
}
