package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.List;

final class OpenAiModelParameterPolicyResolver {
    private final List<OpenAiModelParameterPolicy> policies;

    OpenAiModelParameterPolicyResolver() {
        this(List.of(
                new Gpt5ModelParameterPolicy(),
                new LegacyChatModelParameterPolicy()
        ));
    }

    OpenAiModelParameterPolicyResolver(List<OpenAiModelParameterPolicy> policies) {
        this.policies = policies;
    }

    OpenAiModelParameterPolicy resolve(String model) {
        return policies.stream()
                .filter(policy -> policy.supports(model))
                .findFirst()
                .orElseThrow(() -> new IllegalStateException("No OpenAI model parameter policy for model: " + model));
    }
}
