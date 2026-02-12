package me.go_gradually.omypic.domain.question;

import org.junit.jupiter.api.Test;

import java.time.Instant;
import java.util.List;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class QuestionGroupAggregateTest {

    @Test
    void create_normalizesTags_trimLowercaseDeduplicate() {
        QuestionGroupAggregate group = QuestionGroupAggregate.create(
                "Travel Group",
                List.of(" Travel ", "travel", "HABIT"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        assertEquals(Set.of("travel", "habit"), group.getTags());
    }

    @Test
    void addQuestion_allowsUpToThreeQuestions() {
        QuestionGroupAggregate group = QuestionGroupAggregate.create("g1", List.of("travel"), Instant.now());

        group.addQuestion("q1", "habit", Instant.now());
        group.addQuestion("q2", "compare", Instant.now());
        group.addQuestion("q3", null, Instant.now());

        assertEquals(3, group.getQuestions().size());
        assertThrows(IllegalArgumentException.class, () -> group.addQuestion("q4", null, Instant.now()));
    }

    @Test
    void updateQuestion_updatesTextAndQuestionType() {
        QuestionGroupAggregate group = QuestionGroupAggregate.create("g2", List.of("habit"), Instant.now());
        QuestionItem added = group.addQuestion("original", "habit", Instant.now());

        group.updateQuestion(added.getId(), "updated", "compare", Instant.now());

        assertEquals("updated", group.getQuestions().get(0).getText());
        assertEquals("compare", group.getQuestions().get(0).getQuestionType());
    }

    @Test
    void hasAnyTag_returnsTrueWhenIntersectionExists() {
        QuestionGroupAggregate group = QuestionGroupAggregate.create("g3", List.of("travel", "role-play"), Instant.now());

        assertTrue(group.hasAnyTag(Set.of("travel")));
        assertFalse(group.hasAnyTag(Set.of("habit")));
    }
}
