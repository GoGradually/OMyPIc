package me.go_gradually.omypic.application.feedback.port;

import me.go_gradually.omypic.domain.session.LlmConversationState;
import me.go_gradually.omypic.domain.session.LlmPromptContext;

public interface LlmClient {
    String provider();

    /**
     * A stateless provider keeps no conversation on its side: every call must carry the full
     * system prompt, and bootstrap is skipped. Providers that hold the conversation on the
     * server (OpenAI Responses) keep the default.
     */
    default boolean isStateless() {
        return false;
    }

    LlmConversationState bootstrap(String apiKey,
                                   String model,
                                   String systemPrompt,
                                   LlmConversationState conversationState) throws Exception;

    LlmGenerateResult generate(String apiKey,
                               String model,
                               String systemPrompt,
                               String userPrompt,
                               LlmConversationState conversationState,
                               LlmPromptContext promptContext) throws Exception;
}
