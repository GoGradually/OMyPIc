package me.go_gradually.omypic.presentation.meta.dto;

import java.util.List;

public record ModelMetaResponse(List<String> feedbackModels,
                                String defaultFeedbackModel,
                                List<FeedbackProviderMeta> feedbackProviders,
                                List<String> voiceSttModels,
                                String defaultVoiceSttModel,
                                List<String> ttsModels,
                                String defaultTtsModel,
                                List<String> voices,
                                String defaultVoice,
                                List<String> feedbackLanguages,
                                String defaultFeedbackLanguage) {

    /**
     * Feedback models grouped by the provider whose API key they need.
     */
    public record FeedbackProviderMeta(String provider, List<String> models, String defaultModel) {
    }
}
