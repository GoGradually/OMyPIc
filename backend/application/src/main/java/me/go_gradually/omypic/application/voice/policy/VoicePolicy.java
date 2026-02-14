package me.go_gradually.omypic.application.voice.policy;

public interface VoicePolicy {
    String voiceSttModel();

    String voiceFeedbackModel();

    String voiceFeedbackLanguage();

    String voiceTtsModel();

    String voiceTtsVoice();

    int voiceSilenceDurationMs();
}
