package me.go_gradually.omypic.presentation.feedback.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import java.util.List;

import static org.hamcrest.Matchers.nullValue;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
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
    void feedback_returnsGeneratedEnvelope() throws Exception {
        Feedback feedback = Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of("e1"));
        when(feedbackUseCase.generateFeedback(eq("api-key"), org.mockito.ArgumentMatchers.any(FeedbackCommand.class)))
                .thenReturn(FeedbackResult.generated(feedback));

        mockMvc.perform(post("/api/feedback")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(validRequest())))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.generated").value(true))
                .andExpect(jsonPath("$.feedback.summary").value("summary"))
                .andExpect(jsonPath("$.feedback.correctionPoints[0]").value("p1"));

        ArgumentCaptor<FeedbackCommand> captor = ArgumentCaptor.forClass(FeedbackCommand.class);
        verify(feedbackUseCase).generateFeedback(eq("api-key"), captor.capture());
        assertEquals("s1", captor.getValue().getSessionId());
        assertEquals("text", captor.getValue().getText());
        assertEquals("openai", captor.getValue().getProvider());
        assertEquals("gpt-4o-mini", captor.getValue().getModel());
        assertEquals("ko", captor.getValue().getFeedbackLanguage());
    }

    @Test
    void feedback_returnsSkippedEnvelope() throws Exception {
        when(feedbackUseCase.generateFeedback(eq("api-key"), org.mockito.ArgumentMatchers.any(FeedbackCommand.class)))
                .thenReturn(FeedbackResult.skipped());

        mockMvc.perform(post("/api/feedback")
                        .header("X-API-Key", "api-key")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(validRequest())))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.generated").value(false))
                .andExpect(jsonPath("$.feedback").value(nullValue()));
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
