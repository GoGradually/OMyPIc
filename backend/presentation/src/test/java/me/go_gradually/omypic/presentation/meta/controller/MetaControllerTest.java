package me.go_gradually.omypic.presentation.meta.controller;

import me.go_gradually.omypic.application.voice.policy.VoicePolicy;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.test.web.servlet.MockMvc;

import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, MetaController.class})
@AutoConfigureMockMvc
class MetaControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @MockBean
    private VoicePolicy voicePolicy;

    @Test
    void models_returnsSupportedModelsAndConfiguredDefaults() throws Exception {
        when(voicePolicy.voiceFeedbackModel()).thenReturn("gpt-5-nano");
        when(voicePolicy.voiceSttModel()).thenReturn("gpt-4o-mini-transcribe");
        when(voicePolicy.voiceTtsModel()).thenReturn("gpt-4o-mini-tts");
        when(voicePolicy.voiceTtsVoice()).thenReturn("alloy");
        when(voicePolicy.voiceFeedbackLanguage()).thenReturn("ko");

        mockMvc.perform(get("/api/meta/models"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.feedbackModels[0]").value("gpt-5-mini"))
                .andExpect(jsonPath("$.defaultFeedbackModel").value("gpt-5-nano"))
                .andExpect(jsonPath("$.voiceSttModels[0]").value("gpt-4o-transcribe"))
                .andExpect(jsonPath("$.defaultVoiceSttModel").value("gpt-4o-mini-transcribe"))
                .andExpect(jsonPath("$.ttsModels[0]").value("gpt-4o-mini-tts"))
                .andExpect(jsonPath("$.defaultTtsModel").value("gpt-4o-mini-tts"))
                .andExpect(jsonPath("$.voices[0]").value("alloy"))
                .andExpect(jsonPath("$.defaultVoice").value("alloy"))
                .andExpect(jsonPath("$.feedbackLanguages[0]").value("ko"))
                .andExpect(jsonPath("$.defaultFeedbackLanguage").value("ko"));
    }

    @Test
    void models_fallsBackToFirstSupportedWhenConfiguredValueIsUnknown() throws Exception {
        when(voicePolicy.voiceFeedbackModel()).thenReturn("unknown-model");
        when(voicePolicy.voiceSttModel()).thenReturn("unknown-stt");
        when(voicePolicy.voiceTtsModel()).thenReturn("unknown-tts");
        when(voicePolicy.voiceTtsVoice()).thenReturn("unknown-voice");
        when(voicePolicy.voiceFeedbackLanguage()).thenReturn("unknown-lang");

        mockMvc.perform(get("/api/meta/models"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.defaultFeedbackModel").value("gpt-5-mini"))
                .andExpect(jsonPath("$.defaultVoiceSttModel").value("gpt-4o-transcribe"))
                .andExpect(jsonPath("$.defaultTtsModel").value("gpt-4o-mini-tts"))
                .andExpect(jsonPath("$.defaultVoice").value("alloy"))
                .andExpect(jsonPath("$.defaultFeedbackLanguage").value("ko"));
    }
}
