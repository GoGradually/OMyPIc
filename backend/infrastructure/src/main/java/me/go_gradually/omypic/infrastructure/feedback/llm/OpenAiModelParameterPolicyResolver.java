package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;

final class OpenAiModelParameterPolicyResolver {
    private final List<OpenAiModelParameterPolicy> policies;
    private final Set<String> retryRemovableParameters;

    OpenAiModelParameterPolicyResolver() {
        this(List.of(
                new Gpt5ModelParameterPolicy(),
                new LegacyChatModelParameterPolicy()
        ));
    }

    OpenAiModelParameterPolicyResolver(List<OpenAiModelParameterPolicy> policies) {
        this.policies = policies;
        this.retryRemovableParameters = Set.copyOf(policies.stream()
                .flatMap(policy -> policy.retryRemovableParameters().stream())
                .collect(Collectors.toSet()));
    }

    OpenAiModelParameterPolicy resolve(String model) {
        return policies.stream()
                .filter(policy -> policy.supports(model))
                .findFirst()
                .orElseThrow(() -> new IllegalStateException("No OpenAI model parameter policy for model: " + model));
    }

    boolean isRetryRemovableParameter(String parameterName) {
        if (parameterName == null || parameterName.isBlank()) {
            return false;
        }
        return retryRemovableParameters.contains(parameterName.toLowerCase(Locale.ROOT));
    }

    Set<String> retryRemovableParameters() {
        return retryRemovableParameters;
    }
}
