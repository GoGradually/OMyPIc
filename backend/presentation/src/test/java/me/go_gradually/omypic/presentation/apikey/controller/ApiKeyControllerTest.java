package me.go_gradually.omypic.presentation.apikey.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyResult;
import me.go_gradually.omypic.application.apikey.usecase.ApiKeyVerifyUseCase;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import java.time.Instant;
import java.util.Map;

import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, ApiKeyController.class})
@AutoConfigureMockMvc
class ApiKeyControllerTest {

    @Autowired
    private MockMvc mockMvc;
    @Autowired
    private ObjectMapper objectMapper;

    @MockBean
    private ApiKeyVerifyUseCase useCase;

    @Test
    void verify_returnsResultEnvelope() throws Exception {
        ApiKeyVerifyResult result = ApiKeyVerifyResult.success("openai");
        result.setCheckedAt(Instant.parse("2026-02-10T00:00:00Z"));
        when(useCase.verify(any())).thenReturn(result);

        mockMvc.perform(post("/api/keys/verify")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "provider", "openai",
                                "apiKey", "sk-test",
                                "model", "gpt-4o-mini"
                        ))))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.valid").value(true))
                .andExpect(jsonPath("$.provider").value("openai"));
    }

    @Test
    void verify_returnsBadRequest_whenMissingFields() throws Exception {
        mockMvc.perform(post("/api/keys/verify")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("provider", "openai"))))
                .andExpect(status().isBadRequest());
    }
}
