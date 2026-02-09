package me.go_gradually.omypic.presentation.wrongnote.controller;

import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteId;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.test.web.servlet.MockMvc;

import java.time.Instant;
import java.util.List;

import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, WrongNoteController.class})
@AutoConfigureMockMvc
class WrongNoteControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @MockBean
    private WrongNoteUseCase wrongNoteUseCase;

    @Test
    void list_returnsMappedResponse() throws Exception {
        WrongNote note = WrongNote.rehydrate(
                WrongNoteId.of("n1"),
                "pattern",
                3,
                "summary",
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(wrongNoteUseCase.list()).thenReturn(List.of(note));

        mockMvc.perform(get("/api/wrongnotes"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$[0].id").value("n1"))
                .andExpect(jsonPath("$[0].pattern").value("pattern"))
                .andExpect(jsonPath("$[0].count").value(3))
                .andExpect(jsonPath("$[0].shortSummary").value("summary"));
    }
}
