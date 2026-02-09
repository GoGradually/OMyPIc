package me.go_gradually.omypic.domain.feedback;

import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class FeedbackTest {

    @Test
    void normalized_fillsCorrectionPointsToThree() {
        Feedback feedback = Feedback.of("summary", List.of("Grammar: tense"), "example text", List.of());
        FeedbackConstraints constraints = new FeedbackConstraints(255, 0.8, 1.2);

        Feedback normalized = feedback.normalized(constraints, "user answer text", FeedbackLanguage.of("en"), List.of());

        assertEquals(3, normalized.getCorrectionPoints().size());
        assertTrue(normalized.getCorrectionPoints().get(0).contains("Grammar"));
        assertTrue(normalized.getCorrectionPoints().get(1).contains("Expression"));
        assertTrue(normalized.getCorrectionPoints().get(2).contains("Logic"));
    }

    @Test
    void normalized_trimsSummaryToMaxChars() {
        Feedback feedback = Feedback.of("1234567890", List.of("Grammar", "Expression", "Logic"), "example", List.of());
        FeedbackConstraints constraints = new FeedbackConstraints(5, 0.8, 1.2);

        Feedback normalized = feedback.normalized(constraints, "12345", FeedbackLanguage.of("ko"), List.of());

        assertEquals("1234", normalized.getSummary());
    }

    @Test
    void normalized_trimsExampleWhenRatioIsTooLarge() {
        Feedback feedback = Feedback.of("summary", List.of("Grammar", "Expression", "Logic"), "x".repeat(30), List.of());
        FeedbackConstraints constraints = new FeedbackConstraints(255, 0.8, 1.2);

        Feedback normalized = feedback.normalized(constraints, "1234567890", FeedbackLanguage.of("en"), List.of());

        assertTrue(normalized.getExampleAnswer().length() <= 12);
    }

    @Test
    void normalized_appendsFillerWhenExampleTooShort() {
        Feedback feedback = Feedback.of("summary", List.of("Grammar", "Expression", "Logic"), "짧다", List.of());
        FeedbackConstraints constraints = new FeedbackConstraints(255, 0.8, 1.2);

        Feedback normalized = feedback.normalized(constraints, "충분히 긴 사용자 답변 텍스트", FeedbackLanguage.of("ko"), List.of());

        assertTrue(normalized.getExampleAnswer().contains("또한 하나의 디테일을 더 추가해 보세요."));
    }

    @Test
    void normalized_injectsRulebookEvidence_whenContextExistsAndEvidenceIsEmpty() {
        Feedback feedback = Feedback.of("summary", List.of("Grammar", "Expression", "Logic"), "example answer", List.of());
        FeedbackConstraints constraints = new FeedbackConstraints(255, 0.8, 1.2);
        RulebookContext context = RulebookContext.of(RulebookId.of("r1"), "rulebook.md", "Evidence text");

        Feedback normalized = feedback.normalized(constraints, "user answer", FeedbackLanguage.of("en"), List.of(context));

        assertEquals(1, normalized.getRulebookEvidence().size());
        assertTrue(normalized.getRulebookEvidence().get(0).startsWith("[rulebook.md]"));
    }

    @Test
    void normalized_clearsEvidence_whenContextIsEmpty() {
        Feedback feedback = Feedback.of("summary", List.of("Grammar", "Expression", "Logic"), "example answer", List.of("old evidence"));
        FeedbackConstraints constraints = new FeedbackConstraints(255, 0.8, 1.2);

        Feedback normalized = feedback.normalized(constraints, "user answer", FeedbackLanguage.of("en"), List.of());

        assertTrue(normalized.getRulebookEvidence().isEmpty());
    }
}
