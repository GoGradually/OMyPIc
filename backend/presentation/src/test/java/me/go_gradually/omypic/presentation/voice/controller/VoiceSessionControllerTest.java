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
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
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

    @Test
    void recover_returnsSnapshot() throws Exception {
        VoiceSessionUseCase.RecoverySnapshot snapshot = new VoiceSessionUseCase.RecoverySnapshot(
                "s1",
                "voice-1",
                true,
                false,
                "",
                3L,
                new VoiceSessionUseCase.RecoveryQuestion("q-1", "question", "travel", "g-1", "OPEN"),
                false,
                false,
                9L,
                12L,
                10L,
                false
        );
        when(voiceSessionUseCase.recover("voice-1", 7L)).thenReturn(snapshot);

        mockMvc.perform(get("/api/voice/sessions/voice-1/recovery")
                        .param("lastSeenEventId", "7"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.sessionId").value("s1"))
                .andExpect(jsonPath("$.voiceSessionId").value("voice-1"))
                .andExpect(jsonPath("$.active").value(true))
                .andExpect(jsonPath("$.stopped").value(false))
                .andExpect(jsonPath("$.currentTurnId").value(3))
                .andExpect(jsonPath("$.currentQuestion.id").value("q-1"))
                .andExpect(jsonPath("$.latestEventId").value(12))
                .andExpect(jsonPath("$.replayFromEventId").value(10))
                .andExpect(jsonPath("$.gapDetected").value(false));
    }

    @Test
    void events_passesSinceEventIdToUseCase() throws Exception {
        mockMvc.perform(get("/api/voice/sessions/voice-1/events")
                        .param("sinceEventId", "11"))
                .andExpect(status().isOk());

        verify(voiceSessionUseCase).registerSink(eq("voice-1"), any(), eq(11L));
    }
}
