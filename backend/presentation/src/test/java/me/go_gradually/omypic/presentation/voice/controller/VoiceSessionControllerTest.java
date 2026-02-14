package me.go_gradually.omypic.presentation.voice.controller;

import me.go_gradually.omypic.application.voice.model.VoiceAudioChunkCommand;
import me.go_gradually.omypic.application.voice.model.VoiceSessionOpenCommand;
import me.go_gradually.omypic.application.voice.model.VoiceSessionStopCommand;
import me.go_gradually.omypic.application.voice.usecase.VoiceSessionUseCase;
import me.go_gradually.omypic.presentation.TestBootApplication;
import me.go_gradually.omypic.presentation.shared.error.ApiExceptionHandler;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, VoiceSessionController.class, ApiExceptionHandler.class})
@AutoConfigureMockMvc
class VoiceSessionControllerTest {
    @Autowired
    private MockMvc mockMvc;

    @MockBean
    private VoiceSessionUseCase voiceSessionUseCase;

    @Test
    void open_returnsVoiceSessionId() throws Exception {
        when(voiceSessionUseCase.open(any())).thenReturn("voice-1");

        mockMvc.perform(post("/api/voice/sessions")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content("""
                                {
                                  "sessionId":"s1",
                                  "feedbackModel":"gpt-4o-mini",
                                  "feedbackLanguage":"ko",
                                  "sttModel":"gpt-4o-mini-transcribe",
                                  "ttsVoice":"alloy"
                                }
                                """))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.voiceSessionId").value("voice-1"));

        ArgumentCaptor<VoiceSessionOpenCommand> captor = ArgumentCaptor.forClass(VoiceSessionOpenCommand.class);
        verify(voiceSessionUseCase).open(captor.capture());
        assertEquals("s1", captor.getValue().getSessionId());
        assertEquals("api-key", captor.getValue().getApiKey());
        assertEquals("gpt-4o-mini", captor.getValue().getFeedbackModel());
        assertEquals("ko", captor.getValue().getFeedbackLanguage());
        assertEquals("gpt-4o-mini-transcribe", captor.getValue().getSttModel());
        assertEquals("alloy", captor.getValue().getTtsVoice());
    }

    @Test
    void appendAudio_returnsNoContent() throws Exception {
        mockMvc.perform(post("/api/voice/sessions/voice-1/audio-chunks")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content("""
                                {
                                  "pcm16Base64":"AQI=",
                                  "sampleRate":16000,
                                  "sequence":9
                                }
                                """))
                .andExpect(status().isNoContent());

        ArgumentCaptor<VoiceAudioChunkCommand> captor = ArgumentCaptor.forClass(VoiceAudioChunkCommand.class);
        verify(voiceSessionUseCase).appendAudio(captor.capture());
        assertEquals("voice-1", captor.getValue().getVoiceSessionId());
        assertEquals("AQI=", captor.getValue().getPcm16Base64());
        assertEquals(16000, captor.getValue().getSampleRate());
        assertEquals(9L, captor.getValue().getSequence());
    }

    @Test
    void stop_withoutBody_defaultsToForcedUserStop() throws Exception {
        mockMvc.perform(post("/api/voice/sessions/voice-1/stop"))
                .andExpect(status().isNoContent());

        ArgumentCaptor<VoiceSessionStopCommand> captor = ArgumentCaptor.forClass(VoiceSessionStopCommand.class);
        verify(voiceSessionUseCase).stop(captor.capture());
        assertEquals("voice-1", captor.getValue().getVoiceSessionId());
        assertEquals(true, captor.getValue().isForced());
        assertEquals("user_stop", captor.getValue().getReason());
    }

    @Test
    void open_returnsBadRequest_whenFeedbackModelUnsupported() throws Exception {
        when(voiceSessionUseCase.open(any())).thenThrow(new IllegalArgumentException("Unsupported feedback model: gpt-5.2"));

        mockMvc.perform(post("/api/voice/sessions")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content("""
                                {
                                  "sessionId":"s1",
                                  "feedbackModel":"gpt-5.2"
                                }
                                """))
                .andExpect(status().isBadRequest());
    }
}
