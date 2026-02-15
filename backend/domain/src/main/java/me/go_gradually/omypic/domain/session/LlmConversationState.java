package me.go_gradually.omypic.domain.session;

public record LlmConversationState(String conversationId, String responseId, int turnCountSinceRebase) {
    public LlmConversationState {
        conversationId = normalize(conversationId);
        responseId = normalize(responseId);
        turnCountSinceRebase = Math.max(0, turnCountSinceRebase);
    }

    public static LlmConversationState empty() {
        return new LlmConversationState("", "", 0);
    }

    public boolean hasConversationId() {
        return !conversationId.isBlank();
    }

    private static String normalize(String value) {
        return value == null ? "" : value.trim();
    }
}
