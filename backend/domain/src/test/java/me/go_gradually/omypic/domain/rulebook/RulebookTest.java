package me.go_gradually.omypic.domain.rulebook;

import me.go_gradually.omypic.domain.question.QuestionGroup;
import org.junit.jupiter.api.Test;

import java.time.Instant;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class RulebookTest {

    @Test
    void create_rejectsQuestionGroupForMainScope() {
        assertThrows(IllegalArgumentException.class, () ->
                Rulebook.create("main.md", "/tmp/main.md", RulebookScope.MAIN, QuestionGroup.of("A"), Instant.now()));
    }

    @Test
    void create_requiresQuestionGroupForQuestionScope() {
        assertThrows(IllegalArgumentException.class, () ->
                Rulebook.create("q.md", "/tmp/q.md", RulebookScope.QUESTION, null, Instant.now()));
    }

    @Test
    void create_acceptsValidScopeAndGroupCombination() {
        Rulebook rulebook = Rulebook.create(
                "q.md",
                "/tmp/q.md",
                RulebookScope.QUESTION,
                QuestionGroup.of("A"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        assertEquals(RulebookScope.QUESTION, rulebook.getScope());
        assertEquals(QuestionGroup.of("A"), rulebook.getQuestionGroup());
    }
}
