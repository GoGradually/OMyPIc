package me.go_gradually.omypic.presentation.tts.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.tts.model.AudioSink;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.usecase.TtsUseCase;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import java.nio.charset.StandardCharsets;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.verify;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, TtsController.class})
@AutoConfigureMockMvc
class TtsControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private ObjectMapper objectMapper;

    @MockBean
    private TtsUseCase ttsUseCase;

    @Test
    void stream_setsAudioResponseAndDelegatesToUseCase() throws Exception {
        doAnswer(invocation -> {
            AudioSink sink = invocation.getArgument(2);
            sink.write("abc".getBytes(StandardCharsets.UTF_8));
            return null;
        }).when(ttsUseCase).stream(eq("api-key"), any(TtsCommand.class), any(AudioSink.class));

        mockMvc.perform(post("/api/tts/stream")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("text", "hello", "voice", "alloy"))))
                .andExpect(status().isOk())
                .andExpect(content().contentTypeCompatibleWith(MediaType.valueOf("audio/mpeg")))
                .andExpect(content().bytes("abc".getBytes(StandardCharsets.UTF_8)));

        ArgumentCaptor<TtsCommand> captor = ArgumentCaptor.forClass(TtsCommand.class);
        verify(ttsUseCase).stream(eq("api-key"), captor.capture(), any(AudioSink.class));
        assertEquals("hello", captor.getValue().getText());
        assertEquals("alloy", captor.getValue().getVoice());
    }

    @Test
    void stream_returnsBadRequest_whenValidationFails() throws Exception {
        mockMvc.perform(post("/api/tts/stream")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("text", "", "voice", "alloy"))))
                .andExpect(status().isBadRequest());
    }
}
