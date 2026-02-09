package me.go_gradually.omypic.presentation.session.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
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

import java.time.Instant;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;
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
    void update_usesUpdateMode_andMapsMockExamState() throws Exception {
        SessionState state = buildStateForResponse();
        when(sessionUseCase.updateMode(any(ModeUpdateCommand.class))).thenReturn(state);

        mockMvc.perform(put("/api/modes")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of(
                                "sessionId", "s2",
                                "listId", "list-1",
                                "mode", "MOCK_EXAM",
                                "continuousBatchSize", 5,
                                "mockGroupOrder", List.of("A"),
                                "mockGroupCounts", Map.of("A", 1)
                        ))))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.sessionId").value("s2"))
                .andExpect(jsonPath("$.mode").value("MOCK_EXAM"))
                .andExpect(jsonPath("$.listIndices['list-1']").value(1))
                .andExpect(jsonPath("$.mockExamState.groupOrder[0]").value("A"))
                .andExpect(jsonPath("$.mockExamState.groupCounts.A").value(1));

        ArgumentCaptor<ModeUpdateCommand> captor = ArgumentCaptor.forClass(ModeUpdateCommand.class);
        verify(sessionUseCase).updateMode(captor.capture());
        assertEquals("s2", captor.getValue().getSessionId());
        assertEquals("list-1", captor.getValue().getListId());
        assertEquals(ModeType.MOCK_EXAM, captor.getValue().getMode());
        assertEquals(5, captor.getValue().getContinuousBatchSize());
        assertEquals(List.of("A"), captor.getValue().getMockGroupOrder());
        assertEquals(Map.of("A", 1), captor.getValue().getMockGroupCounts());
    }

    private SessionState buildStateForResponse() {
        SessionState state = new SessionState(SessionId.of("s2"));
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "List",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Question", "A")),
                Instant.parse("2026-02-01T00:00:00Z"),
                Instant.parse("2026-02-01T00:00:00Z")
        );

        state.nextQuestion(list);
        state.applyModeUpdate(ModeType.MOCK_EXAM, 3);
        state.configureMockExam(list, List.of("A"), Map.of("A", 1));
        return state;
    }
}
