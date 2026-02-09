package me.go_gradually.omypic.presentation.question.controller;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
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
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("l1"),
                "My List",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A")),
                Instant.parse("2026-02-01T00:00:00Z"),
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(questionUseCase.list()).thenReturn(List.of(list));

        mockMvc.perform(get("/api/questions"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$[0].id").value("l1"))
                .andExpect(jsonPath("$[0].name").value("My List"))
                .andExpect(jsonPath("$[0].questions[0].id").value("q1"));
    }

    @Test
    void create_usesUntitledDefault_whenNameMissing() throws Exception {
        QuestionList created = QuestionList.rehydrate(
                QuestionListId.of("l2"),
                "Untitled",
                List.of(),
                Instant.parse("2026-02-02T00:00:00Z"),
                Instant.parse("2026-02-02T00:00:00Z")
        );
        when(questionUseCase.create("Untitled")).thenReturn(created);

        mockMvc.perform(post("/api/questions")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of())))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.id").value("l2"))
                .andExpect(jsonPath("$.name").value("Untitled"));

        verify(questionUseCase).create("Untitled");
    }

    @Test
    void rename_usesUntitledDefault_whenNameMissing() throws Exception {
        QuestionList renamed = QuestionList.rehydrate(
                QuestionListId.of("l3"),
                "Untitled",
                List.of(),
                Instant.parse("2026-02-03T00:00:00Z"),
                Instant.parse("2026-02-03T00:00:00Z")
        );
        when(questionUseCase.updateName("l3", "Untitled")).thenReturn(renamed);

        mockMvc.perform(put("/api/questions/l3")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of())))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.name").value("Untitled"));

        verify(questionUseCase).updateName("l3", "Untitled");
    }

    @Test
    void nextQuestion_mapsUseCaseResponse() throws Exception {
        NextQuestion next = new NextQuestion();
        next.setQuestionId("q9");
        next.setText("What?");
        next.setGroup("B");
        next.setSkipped(false);
        next.setMockExamCompleted(true);
        next.setMockExamCompletionReason("QUESTION_EXHAUSTED");
        when(questionUseCase.nextQuestion("list-1", "s1")).thenReturn(next);

        mockMvc.perform(get("/api/questions/list-1/next").param("sessionId", "s1"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.questionId").value("q9"))
                .andExpect(jsonPath("$.text").value("What?"))
                .andExpect(jsonPath("$.group").value("B"))
                .andExpect(jsonPath("$.skipped").value(false))
                .andExpect(jsonPath("$.mockExamCompleted").value(true))
                .andExpect(jsonPath("$.mockExamCompletionReason").value("QUESTION_EXHAUSTED"));
    }

    @Test
    void addQuestion_passesBodyToUseCase() throws Exception {
        QuestionList updated = QuestionList.rehydrate(
                QuestionListId.of("l4"),
                "List",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q2"), "new question", "G")),
                Instant.parse("2026-02-04T00:00:00Z"),
                Instant.parse("2026-02-04T00:00:00Z")
        );
        when(questionUseCase.addQuestion(eq("l4"), eq("new question"), eq("G"))).thenReturn(updated);

        mockMvc.perform(post("/api/questions/l4/items")
                        .contentType(MediaType.APPLICATION_JSON)
                        .content(objectMapper.writeValueAsString(Map.of("text", "new question", "group", "G"))))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.questions[0].id").value("q2"));

        ArgumentCaptor<String> textCaptor = ArgumentCaptor.forClass(String.class);
        ArgumentCaptor<String> groupCaptor = ArgumentCaptor.forClass(String.class);
        verify(questionUseCase).addQuestion(eq("l4"), textCaptor.capture(), groupCaptor.capture());
        assertEquals("new question", textCaptor.getValue());
        assertEquals("G", groupCaptor.getValue());
    }
}
