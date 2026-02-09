package me.go_gradually.omypic.presentation.tts.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.tts.usecase.TtsUseCase;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import java.util.Map;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
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
    void stream_returnsGone_whenRealtimeOnlyEnabled() throws Exception {
        mockMvc.perform(post("/api/tts/stream")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("text", "hello", "voice", "alloy"))))
                .andExpect(status().isGone());
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
