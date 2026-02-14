package me.go_gradually.omypic.presentation.stt.controller;

import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.usecase.SttJobUseCase;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.mock.web.MockMultipartFile;
import org.springframework.test.web.servlet.MockMvc;

import java.nio.charset.StandardCharsets;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.*;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.multipart;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, SttController.class})
@AutoConfigureMockMvc
class SttControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @MockBean
    private SttUseCase sttUseCase;

    @MockBean
    private SttJobUseCase sttJobUseCase;

    @MockBean
    private SessionUseCase sessionUseCase;

    @Test
    void upload_returnsText_forNonStreamRequest() throws Exception {
        when(sttUseCase.transcribe(any())).thenReturn("transcribed-text");

        mockMvc.perform(multipart("/api/stt/upload")
                        .file(new MockMultipartFile("file", "a.webm", "audio/webm", "audio".getBytes(StandardCharsets.UTF_8)))
                        .header("X-API-Key", "api-key")
                        .param("sessionId", "s1"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.text").value("transcribed-text"));

        ArgumentCaptor<SttCommand> captor = ArgumentCaptor.forClass(SttCommand.class);
        verify(sttUseCase).transcribe(captor.capture());
        verify(sessionUseCase).appendSegment("s1", "transcribed-text");
        verify(sttJobUseCase, never()).createJob(any());
        assertEquals("api-key", captor.getValue().getApiKey());
        assertEquals("gpt-4o-mini-transcribe", captor.getValue().getModel());
        assertTrue(captor.getValue().getFileBytes().length > 0);
        assertEquals(false, captor.getValue().isTranslate());
        assertEquals("s1", captor.getValue().getSessionId());
    }

    @Test
    void upload_streamTrue_returnsJobId() throws Exception {
        when(sttJobUseCase.createJob(any())).thenReturn("job-1");

        mockMvc.perform(multipart("/api/stt/upload")
                        .file(new MockMultipartFile("file", "a.webm", "audio/webm", "audio".getBytes(StandardCharsets.UTF_8)))
                        .header("X-API-Key", "api-key")
                        .param("stream", "true")
                        .param("model", "whisper-1")
                        .param("translate", "true"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.jobId").value("job-1"));

        ArgumentCaptor<SttCommand> captor = ArgumentCaptor.forClass(SttCommand.class);
        verify(sttJobUseCase).createJob(captor.capture());
        verify(sttUseCase, never()).transcribe(any());
        verify(sessionUseCase, never()).appendSegment(any(), any());
        assertEquals("api-key", captor.getValue().getApiKey());
        assertEquals("whisper-1", captor.getValue().getModel());
        assertEquals(true, captor.getValue().isTranslate());
    }

    @Test
    void upload_returnsBadRequest_whenApiKeyMissing() throws Exception {
        mockMvc.perform(multipart("/api/stt/upload")
                        .file(new MockMultipartFile("file", "a.webm", "audio/webm", "audio".getBytes(StandardCharsets.UTF_8))))
                .andExpect(status().isBadRequest());
    }

    @Test
    void stream_registersSinkAndReturnsEventStream() throws Exception {
        mockMvc.perform(get("/api/stt/stream").param("jobId", "job-1"))
                .andExpect(status().isOk());
        verify(sttJobUseCase).registerSink(eq("job-1"), any());
    }
}
