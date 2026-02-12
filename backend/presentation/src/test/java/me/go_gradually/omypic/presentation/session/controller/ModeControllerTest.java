package me.go_gradually.omypic.presentation.session.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, ModeController.class})
@AutoConfigureMockMvc
class ModeControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private ObjectMapper objectMapper;

    @MockBean
    private SessionUseCase sessionUseCase;

    @Test
    void update_usesGetOrCreate_whenListIdIsNull() throws Exception {
        SessionState state = new SessionState(SessionId.of("s1"));
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);

        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("sessionId", "s1"))))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.sessionId").value("s1"))
                .andExpect(jsonPath("$.mode").value("IMMEDIATE"));

        verify(sessionUseCase).getOrCreate("s1");
        verify(sessionUseCase, never()).updateMode(any(ModeUpdateCommand.class));
    }

    @Test
    void update_usesUpdateMode_withoutMockExamFields() throws Exception {
        SessionState state = new SessionState(SessionId.of("s2"));
        state.applyModeUpdate(ModeType.CONTINUOUS, 5);
        state.setActiveQuestionListId("list-1");
        when(sessionUseCase.updateMode(any(ModeUpdateCommand.class))).thenReturn(state);

        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "sessionId", "s2",
                                "listId", "list-1",
                                "mode", "CONTINUOUS",
                                "continuousBatchSize", 5
                        ))))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.sessionId").value("s2"))
                .andExpect(jsonPath("$.mode").value("CONTINUOUS"))
                .andExpect(jsonPath("$.continuousBatchSize").value(5))
                .andExpect(jsonPath("$.activeListId").value("list-1"))
                .andExpect(jsonPath("$.mockExamState").doesNotExist());

        ArgumentCaptor<ModeUpdateCommand> captor = ArgumentCaptor.forClass(ModeUpdateCommand.class);
        verify(sessionUseCase).updateMode(captor.capture());
        assertEquals("s2", captor.getValue().getSessionId());
        assertEquals("list-1", captor.getValue().getListId());
        assertEquals(ModeType.CONTINUOUS, captor.getValue().getMode());
        assertEquals(5, captor.getValue().getContinuousBatchSize());
    }

    @Test
    void update_returnsBadRequest_whenModeIsUnsupported() throws Exception {
        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "sessionId", "s3",
                                "listId", "list-1",
                                "mode", "MOCK_EXAM"
                        ))))
                .andExpect(status().isBadRequest());
    }
}
