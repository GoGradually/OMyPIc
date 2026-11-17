package me.go_gradually.omypic.application.voice.policy;

import java.util.List;

public final class VoiceModelCatalog {
    /**
     * Speech recognition that runs on the learner's device (no API, no cost).
     * The client transcribes the answer and sends the text; the server never receives audio.
     */
    public static final String LOCAL_STT_MODEL = "local-whisper";
    /**
     * Speech synthesis by the device's built-in voice (no API, no cost).
     * The server sends the text to speak instead of audio.
     */
    public static final String DEVICE_TTS_MODEL = "device-voice";

    private static final List<String> STT_MODELS = List.of(
            "gpt-4o-transcribe",
            "gpt-4o-mini-transcribe",
            "whisper-1",
            LOCAL_STT_MODEL
    );
    private static final List<String> TTS_MODELS = List.of(
            "gpt-4o-mini-tts",
            "tts-1-hd",
            "tts-1",
            DEVICE_TTS_MODEL
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

    public static boolean isLocalStt(String model) {
        return LOCAL_STT_MODEL.equalsIgnoreCase(model == null ? "" : model.trim());
    }

    public static boolean isDeviceTts(String model) {
        return DEVICE_TTS_MODEL.equalsIgnoreCase(model == null ? "" : model.trim());
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
