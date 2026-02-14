package me.go_gradually.omypic.infrastructure.feedback.llm;

import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;

final class OpenAiLlmLogFormatter {
    private static final int DEFAULT_RESPONSE_PREVIEW_CHARS = 1024;
    private static final boolean DEFAULT_FULL_BODY = false;
    private static final boolean DEFAULT_LOG_SUCCESS_AT_FINE = true;

    private final int responsePreviewChars;
    private final boolean fullBody;
    private final boolean logSuccessAtFine;

    OpenAiLlmLogFormatter(int responsePreviewChars, boolean fullBody, boolean logSuccessAtFine) {
        this.responsePreviewChars = Math.max(0, responsePreviewChars);
        this.fullBody = fullBody;
        this.logSuccessAtFine = logSuccessAtFine;
    }

    static OpenAiLlmLogFormatter from(AppProperties properties) {
        if (properties == null || properties.getIntegrations() == null || properties.getIntegrations().getOpenai() == null) {
            return defaults();
        }
        AppProperties.OpenAi.Logging logging = properties.getIntegrations().getOpenai().getLogging();
        if (logging == null) {
            return defaults();
        }
        return new OpenAiLlmLogFormatter(
                logging.getResponsePreviewChars(),
                logging.isFullBody(),
                logging.isLogSuccessAtFine()
        );
    }

    static OpenAiLlmLogFormatter defaults() {
        return new OpenAiLlmLogFormatter(
                DEFAULT_RESPONSE_PREVIEW_CHARS,
                DEFAULT_FULL_BODY,
                DEFAULT_LOG_SUCCESS_AT_FINE
        );
    }

    boolean shouldLogSuccessAtFine() {
        return logSuccessAtFine;
    }

    String responsePreview(String body) {
        if (body == null || body.isBlank()) {
            return "";
        }
        String normalized = normalizeForLog(body);
        if (fullBody) {
            return normalized;
        }
        if (responsePreviewChars <= 0 || normalized.length() <= responsePreviewChars) {
            return normalized;
        }
        return normalized.substring(0, responsePreviewChars) + "...(truncated)";
    }

    String optionalParameterSummary(Map<String, Object> payload, Set<String> optionalParameters) {
        if (payload == null || payload.isEmpty() || optionalParameters == null || optionalParameters.isEmpty()) {
            return "[]";
        }
        List<String> included = new ArrayList<>();
        for (String key : optionalParameters) {
            if (payload.containsKey(key)) {
                included.add(key);
            }
        }
        Collections.sort(included);
        return included.toString();
    }

    private String normalizeForLog(String text) {
        return text
                .replace("\r", "\\r")
                .replace("\n", "\\n");
    }
}
