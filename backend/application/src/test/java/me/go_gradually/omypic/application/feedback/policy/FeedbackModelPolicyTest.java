package me.go_gradually.omypic.application.feedback.policy;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class FeedbackModelPolicyTest {

    @Test
    void validateOrThrow_allowsSupportedModel() {
        assertDoesNotThrow(() -> FeedbackModelPolicy.validateOrThrow("gpt-5-mini"));
    }

    @Test
    void validateOrThrow_allowsBlankModelForFallback() {
        assertDoesNotThrow(() -> FeedbackModelPolicy.validateOrThrow(" "));
    }

    @Test
    void validateOrThrow_rejectsUnsupportedModel() {
        IllegalArgumentException error = assertThrows(
                IllegalArgumentException.class,
                () -> FeedbackModelPolicy.validateOrThrow("gpt-5.2")
        );

        assertEquals("Unsupported feedback model: gpt-5.2", error.getMessage());
    }
}
