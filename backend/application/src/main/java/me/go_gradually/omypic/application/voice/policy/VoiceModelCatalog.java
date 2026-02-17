package me.go_gradually.omypic.application.voice.policy;

import java.util.List;

public final class VoiceModelCatalog {
    private static final List<String> STT_MODELS = List.of(
            "gpt-4o-transcribe",
            "gpt-4o-mini-transcribe",
            "whisper-1"
    );
    private static final List<String> TTS_MODELS = List.of(
            "gpt-4o-mini-tts",
            "tts-1-hd",
            "tts-1"
    );
    private static final List<String> TTS_VOICES = List.of(
            "alloy",
            "echo",
            "fable",
            "nova",
            "shimmer"
    );
    private static final List<String> FEEDBACK_LANGUAGES = List.of(
            "ko",
            "en"
    );

    private VoiceModelCatalog() {
    }

    public static List<String> supportedSttModels() {
        return STT_MODELS;
    }

    public static List<String> supportedTtsModels() {
        return TTS_MODELS;
    }

    public static List<String> supportedVoices() {
        return TTS_VOICES;
    }

    public static List<String> supportedFeedbackLanguages() {
        return FEEDBACK_LANGUAGES;
    }
}
