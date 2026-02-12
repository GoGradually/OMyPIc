package me.go_gradually.omypic.presentation.rulebook.controller;

import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.rulebook.RulebookScope;
import me.go_gradually.omypic.presentation.TestBootApplication;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.mock.mockito.MockBean;
import org.springframework.mock.web.MockMultipartFile;
import org.springframework.test.web.servlet.MockMvc;

import java.time.Instant;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.*;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest(classes = {TestBootApplication.class, RulebookController.class})
@AutoConfigureMockMvc
class RulebookControllerTest {

    @Autowired
    private MockMvc mockMvc;

    @MockBean
    private RulebookUseCase rulebookUseCase;

    @Test
    void list_returnsMappedResponse() throws Exception {
        Rulebook rulebook = Rulebook.rehydrate(
                RulebookId.of("r1"),
                "rules.md",
                "/tmp/rules.md",
                RulebookScope.MAIN,
                null,
                true,
                Instant.parse("2026-02-01T00:00:00Z"),
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(rulebookUseCase.list()).thenReturn(List.of(rulebook));

        mockMvc.perform(get("/api/rulebooks"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$[0].id").value("r1"))
                .andExpect(jsonPath("$[0].filename").value("rules.md"))
                .andExpect(jsonPath("$[0].scope").value("MAIN"))
                .andExpect(jsonPath("$[0].enabled").value(true));
    }

    @Test
    void upload_passesMultipartToUseCase() throws Exception {
        Rulebook uploaded = Rulebook.rehydrate(
                RulebookId.of("r2"),
                "rulebook.md",
                "/tmp/rulebook.md",
                RulebookScope.QUESTION,
                QuestionGroup.of("A"),
                true,
                Instant.parse("2026-02-02T00:00:00Z"),
                Instant.parse("2026-02-02T00:00:00Z")
        );
        byte[] bytes = "hello".getBytes();
        when(rulebookUseCase.upload(eq("rulebook.md"), any(byte[].class), eq(RulebookScope.QUESTION), eq("A"))).thenReturn(uploaded);

        mockMvc.perform(multipart("/api/rulebooks")
                        .file(new MockMultipartFile("file", "rulebook.md", "text/markdown", bytes))
                        .param("scope", "QUESTION")
                        .param("questionGroup", "A"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.id").value("r2"))
                .andExpect(jsonPath("$.scope").value("QUESTION"))
                .andExpect(jsonPath("$.questionGroup").value("A"))
                .andExpect(jsonPath("$.path").value("/tmp/rulebook.md"));

        ArgumentCaptor<byte[]> captor = ArgumentCaptor.forClass(byte[].class);
        verify(rulebookUseCase).upload(eq("rulebook.md"), captor.capture(), eq(RulebookScope.QUESTION), eq("A"));
        assertArrayEquals(bytes, captor.getValue());
    }

    @Test
    void toggle_updatesEnabledFlag() throws Exception {
        Rulebook toggled = Rulebook.rehydrate(
                RulebookId.of("r3"),
                "rules.md",
                "/tmp/rules.md",
                RulebookScope.MAIN,
                null,
                false,
                Instant.parse("2026-02-03T00:00:00Z"),
                Instant.parse("2026-02-03T00:00:00Z")
        );
        when(rulebookUseCase.toggle("r3", false)).thenReturn(toggled);

        mockMvc.perform(put("/api/rulebooks/r3/toggle").param("enabled", "false"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.enabled").value(false));
    }

    @Test
    void delete_delegatesToUseCase() throws Exception {
        mockMvc.perform(delete("/api/rulebooks/r4"))
                .andExpect(status().isOk());

        verify(rulebookUseCase).delete("r4");
    }
}
