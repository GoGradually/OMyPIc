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
    /**
     * Speech synthesis by a model that runs on the learner's device (no API, no cost).
     * Like the device voice, the server sends the text to speak instead of audio.
     */
    public static final String LOCAL_TTS_MODEL = "local-kokoro";

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
            DEVICE_TTS_MODEL,
            LOCAL_TTS_MODEL
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

    /** True for TTS models the client speaks itself: the device voice and the on-device model. */
    public static boolean isClientTts(String model) {
        String candidate = model == null ? "" : model.trim();
        return DEVICE_TTS_MODEL.equalsIgnoreCase(candidate) || LOCAL_TTS_MODEL.equalsIgnoreCase(candidate);
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
