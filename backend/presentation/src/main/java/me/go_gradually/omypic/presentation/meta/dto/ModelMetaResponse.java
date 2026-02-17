package me.go_gradually.omypic.presentation.meta.dto;

import java.util.List;

public record ModelMetaResponse(List<String> feedbackModels,
                                String defaultFeedbackModel,
                                List<String> voiceSttModels,
                                String defaultVoiceSttModel,
                                List<String> ttsModels,
                                String defaultTtsModel,
                                List<String> voices,
                                String defaultVoice,
                                List<String> feedbackLanguages,
                                String defaultFeedbackLanguage) {
}
