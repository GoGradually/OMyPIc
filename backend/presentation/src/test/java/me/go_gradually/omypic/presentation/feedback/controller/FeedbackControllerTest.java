package me.go_gradually.omypic.presentation.feedback.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import static org.mockito.Mockito.verifyNoInteractions;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, FeedbackController.class})
@AutoConfigureMockMvc
class FeedbackControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private ObjectMapper objectMapper;

    @MockBean
    private FeedbackUseCase feedbackUseCase;

    @Test
    void feedback_returnsGone_whenRealtimeOnlyEnabled() throws Exception {
        mockMvc.perform(post("/api/feedback")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(validRequest())))
                .andExpect(status().isGone());
        verifyNoInteractions(feedbackUseCase);
    }

    @Test
    void feedback_returnsGone_forValidRequest() throws Exception {
        mockMvc.perform(post("/api/feedback")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(validRequest())))
                .andExpect(status().isGone());
        verifyNoInteractions(feedbackUseCase);
    }

    @Test
    void feedback_returnsBadRequest_whenApiKeyMissing() throws Exception {
        mockMvc.perform(post("/api/feedback")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(validRequest())))
                .andExpect(status().isBadRequest());
    }

    @Test
    void feedback_returnsBadRequest_whenValidationFails() throws Exception {
        var invalid = validRequest();
        invalid.put("text", "");

        mockMvc.perform(post("/api/feedback")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(invalid)))
                .andExpect(status().isBadRequest());
    }

    @Test
    void mockFinalEndpoint_returnsNotFound_afterRemoval() throws Exception {
        mockMvc.perform(post("/api/feedback/mock-final")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(validRequest())))
                .andExpect(status().isNotFound());
    }

    private java.util.Map<String, String> validRequest() {
        java.util.Map<String, String> map = new java.util.HashMap<>();
        map.put("sessionId", "s1");
        map.put("text", "text");
        map.put("provider", "openai");
        map.put("model", "gpt-4o-mini");
        map.put("feedbackLanguage", "ko");
        return map;
    }

}
