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

    @Test
    void validateOrThrow_allowsClaudeModels() {
        assertDoesNotThrow(() -> FeedbackModelPolicy.validateOrThrow("claude-opus-5-5"));
        assertDoesNotThrow(() -> FeedbackModelPolicy.validateOrThrow("anthropic", "claude-haiku-5-5"));
    }

    @Test
    void validateOrThrow_rejectsModelFromAnotherProvider() {
        IllegalArgumentException error = assertThrows(
                IllegalArgumentException.class,
                () -> FeedbackModelPolicy.validateOrThrow("anthropic", "gpt-5-mini")
        );

        assertEquals("Feedback model gpt-5-mini does not belong to provider anthropic", error.getMessage());
    }

    @Test
    void providerOf_resolvesProviderFromModelId() {
        assertEquals("anthropic", FeedbackModelPolicy.providerOf("claude-sonnet-5-5"));
        assertEquals("openai", FeedbackModelPolicy.providerOf("gpt-5-nano"));
        assertEquals("openai", FeedbackModelPolicy.providerOf(" "));
    }

    @Test
    void supportedModels_groupsModelsByProvider() {
        assertEquals("claude-opus-5-5", FeedbackModelPolicy.defaultModel("anthropic"));
        assertEquals(4, FeedbackModelPolicy.supportedModels("anthropic").size());
        assertEquals(7, FeedbackModelPolicy.supportedModels("openai").size());
        assertEquals(11, FeedbackModelPolicy.supportedModels().size());
    }

    @Test
    void normalizeProvider_rejectsUnknownProvider() {
        assertThrows(IllegalArgumentException.class, () -> FeedbackModelPolicy.normalizeProvider("gemini"));
    }
}
