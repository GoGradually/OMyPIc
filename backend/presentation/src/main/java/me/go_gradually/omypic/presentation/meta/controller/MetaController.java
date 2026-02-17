package me.go_gradually.omypic.presentation.meta.controller;

import me.go_gradually.omypic.application.feedback.policy.FeedbackModelPolicy;
import me.go_gradually.omypic.application.voice.policy.VoiceModelCatalog;
import me.go_gradually.omypic.application.voice.policy.VoicePolicy;
import me.go_gradually.omypic.presentation.meta.dto.ModelMetaResponse;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

import java.util.List;

@RestController
@RequestMapping("/api/meta")
public class MetaController {
    private final VoicePolicy voicePolicy;

    public MetaController(VoicePolicy voicePolicy) {
        this.voicePolicy = voicePolicy;
    }

    @GetMapping("/models")
    public ModelMetaResponse models() {
        List<String> feedbackModels = FeedbackModelPolicy.supportedModels();
        List<String> sttModels = VoiceModelCatalog.supportedSttModels();
        List<String> ttsModels = VoiceModelCatalog.supportedTtsModels();
        List<String> voices = VoiceModelCatalog.supportedVoices();
        List<String> feedbackLanguages = VoiceModelCatalog.supportedFeedbackLanguages();
        return new ModelMetaResponse(
                feedbackModels,
                resolveDefault(voicePolicy.voiceFeedbackModel(), feedbackModels),
                sttModels,
                resolveDefault(voicePolicy.voiceSttModel(), sttModels),
                ttsModels,
                resolveDefault(voicePolicy.voiceTtsModel(), ttsModels),
                voices,
                resolveDefault(voicePolicy.voiceTtsVoice(), voices),
                feedbackLanguages,
                resolveDefault(voicePolicy.voiceFeedbackLanguage(), feedbackLanguages)
        );
    }

    private String resolveDefault(String configured, List<String> supported) {
        if (supported == null || supported.isEmpty()) {
            return "";
        }
        if (configured == null || configured.isBlank()) {
            return supported.get(0);
        }
        return supported.contains(configured) ? configured : supported.get(0);
    }
}
