package me.go_gradually.omypic.application.feedback.port;

import me.go_gradually.omypic.domain.session.LlmConversationState;
import me.go_gradually.omypic.domain.session.LlmPromptContext;

public interface LlmClient {
    String provider();

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
