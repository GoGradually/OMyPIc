package me.go_gradually.omypic.presentation.question.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.question.model.QuestionTagStat;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
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
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.*;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, QuestionController.class})
@AutoConfigureMockMvc
class QuestionControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private ObjectMapper objectMapper;

    @MockBean
    private QuestionUseCase questionUseCase;

    @Test
    void list_returnsMappedResponse() throws Exception {
        QuestionGroupAggregate group = QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of("g1"),
                "Travel",
                List.of("travel", "habit"),
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "habit")),
                Instant.parse("2026-02-01T00:00:00Z"),
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(questionUseCase.list()).thenReturn(List.of(group));

        mockMvc.perform(get("/api/question-groups"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$[0].id").value("g1"))
                .andExpect(jsonPath("$[0].name").value("Travel"))
                .andExpect(jsonPath("$[0].tags[0]").value("habit"))
                .andExpect(jsonPath("$[0].questions[0].id").value("q1"));
    }

    @Test
    void create_passesNameAndTagsToUseCase() throws Exception {
        QuestionGroupAggregate created = QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of("g2"),
                "New Group",
                List.of("travel"),
                List.of(),
                Instant.parse("2026-02-02T00:00:00Z"),
                Instant.parse("2026-02-02T00:00:00Z")
        );
        when(questionUseCase.createGroup(eq("New Group"), eq(List.of("travel")))).thenReturn(created);

        mockMvc.perform(post("/api/question-groups")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("name", "New Group", "tags", List.of("travel")))))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.id").value("g2"))
                .andExpect(jsonPath("$.name").value("New Group"));

        verify(questionUseCase).createGroup("New Group", List.of("travel"));
    }

    @Test
    void addQuestion_passesBodyToUseCase() throws Exception {
        QuestionGroupAggregate updated = QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of("g4"),
                "Group",
                List.of("travel"),
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q2"), "new question", "compare")),
                Instant.parse("2026-02-04T00:00:00Z"),
                Instant.parse("2026-02-04T00:00:00Z")
        );
        when(questionUseCase.addQuestion(eq("g4"), eq("new question"), eq("compare"))).thenReturn(updated);

        mockMvc.perform(post("/api/question-groups/g4/items")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("text", "new question", "questionType", "compare"))))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.questions[0].id").value("q2"));

        ArgumentCaptor<String> textCaptor = ArgumentCaptor.forClass(String.class);
        ArgumentCaptor<String> typeCaptor = ArgumentCaptor.forClass(String.class);
        verify(questionUseCase).addQuestion(eq("g4"), textCaptor.capture(), typeCaptor.capture());
        assertEquals("new question", textCaptor.getValue());
        assertEquals("compare", typeCaptor.getValue());
    }

    @Test
    void tagStats_mapsUseCaseResponse() throws Exception {
        when(questionUseCase.listTagStats()).thenReturn(List.of(
                new QuestionTagStat("habit", 1, true),
                new QuestionTagStat("travel", 2, true)
        ));

        mockMvc.perform(get("/api/question-groups/tags/stats"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$[0].tag").value("habit"))
                .andExpect(jsonPath("$[0].groupCount").value(1))
                .andExpect(jsonPath("$[0].selectable").value(true));
    }
}
