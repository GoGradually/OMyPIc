package me.go_gradually.omypic.application.realtime.policy;

public interface RealtimePolicy {
    String realtimeConversationModel();

    String realtimeSttModel();

    String realtimeFeedbackProvider();

    String realtimeFeedbackModel();

    String realtimeFeedbackLanguage();

    String realtimeTtsVoice();
}
