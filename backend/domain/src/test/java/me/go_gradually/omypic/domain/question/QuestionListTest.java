package me.go_gradually.omypic.domain.question;

import org.junit.jupiter.api.Test;

import java.time.Instant;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

class QuestionListTest {

    @Test
    void addQuestion_allowsUpTo200Items() {
        QuestionList list = QuestionList.create("list", Instant.parse("2026-01-01T00:00:00Z"));

        for (int i = 0; i < 200; i++) {
            list.addQuestion("question-" + i, "group", Instant.now());
        }

        assertEquals(200, list.getQuestions().size());
    }

    @Test
    void addQuestion_throwsWhenOver200Items() {
        QuestionList list = QuestionList.create("list", Instant.parse("2026-01-01T00:00:00Z"));
        for (int i = 0; i < 200; i++) {
            list.addQuestion("question-" + i, "group", Instant.now());
        }

        assertThrows(IllegalArgumentException.class,
                () -> list.addQuestion("overflow", "group", Instant.now()));
    }

    @Test
    void rename_updatesNameAndTimestamp() {
        QuestionList list = QuestionList.create("before", Instant.parse("2026-01-01T00:00:00Z"));

        Instant after = Instant.parse("2026-01-01T01:00:00Z");
        list.rename("after", after);

        assertEquals("after", list.getName());
        assertEquals(after, list.getUpdatedAt());
    }

    @Test
    void updateQuestion_updatesMatchingItem() {
        QuestionItem item = QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A");
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "list",
                List.of(item),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        list.updateQuestion(QuestionItemId.of("q1"), "Q1 updated", "B", Instant.parse("2026-01-01T02:00:00Z"));

        assertEquals("Q1 updated", list.getQuestions().get(0).getText());
        assertEquals("B", list.getQuestions().get(0).getGroup());
    }

    @Test
    void updateQuestion_withNullId_keepsQuestionsUntouched() {
        QuestionItem item = QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A");
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "list",
                List.of(item),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        list.updateQuestion(null, "Q1 updated", "B", Instant.parse("2026-01-01T02:00:00Z"));

        assertEquals("Q1", list.getQuestions().get(0).getText());
        assertEquals("A", list.getQuestions().get(0).getGroup());
    }

    @Test
    void removeQuestion_removesMatchingItem() {
        QuestionItem item1 = QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A");
        QuestionItem item2 = QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", "B");
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "list",
                List.of(item1, item2),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        list.removeQuestion(QuestionItemId.of("q1"), Instant.parse("2026-01-01T03:00:00Z"));

        assertEquals(1, list.getQuestions().size());
        assertEquals("q2", list.getQuestions().get(0).getId().value());
    }

    @Test
    void removeQuestion_throwsWhenRemovingLastQuestion() {
        QuestionItem item = QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A");
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "list",
                List.of(item),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        assertThrows(IllegalStateException.class,
                () -> list.removeQuestion(QuestionItemId.of("q1"), Instant.parse("2026-01-01T03:00:00Z")));
    }

    @Test
    void groupQuestionIdsByGroup_excludesNullGroups() {
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "list",
                List.of(
                        QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A"),
                        QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", "A"),
                        QuestionItem.rehydrate(QuestionItemId.of("q3"), "Q3", null)
                ),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        Map<String, List<QuestionItemId>> grouped = list.groupQuestionIdsByGroup();

        assertEquals(1, grouped.size());
        assertEquals(2, grouped.get("A").size());
        assertTrue(grouped.containsKey("A"));
    }
}
