package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.domain.session.LlmConversationState;
import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.domain.session.LlmPromptContext;
import me.go_gradually.omypic.domain.feedback.CorrectionDetail;
import me.go_gradually.omypic.domain.feedback.Corrections;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.RecommendationDetail;
import me.go_gradually.omypic.domain.feedback.Recommendations;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.ai.converter.BeanOutputConverter;
import org.springframework.http.MediaType;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;
import org.springframework.web.reactive.function.client.WebClientResponseException;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.logging.Level;
import java.util.logging.Logger;

@Component
public class OpenAiLlmClient implements LlmClient {
    private static final String DEFAULT_CHAT_MODEL = "gpt-4o-mini";
    private static final String BOOTSTRAP_INPUT_TEXT = "Acknowledge the coaching strategy and wait for the next user answer.";
    private static final Logger log = Logger.getLogger(OpenAiLlmClient.class.getName());

    private final ObjectMapper objectMapper = new ObjectMapper();
    private final OpenAiModelParameterPolicyResolver modelParameterPolicyResolver = new OpenAiModelParameterPolicyResolver();
    private final OpenAiLlmLogFormatter logFormatter;
    private final WebClient webClient;
    private final boolean responsesEnabled;

    public OpenAiLlmClient(AppProperties properties) {
        this.logFormatter = OpenAiLlmLogFormatter.from(properties);
        this.webClient = WebClient.builder()
                .baseUrl(properties.getIntegrations().getOpenai().getBaseUrl())
                .build();
        this.responsesEnabled = properties.getIntegrations().getOpenai().isResponsesEnabled();
    }

    @Override
    public String provider() {
        return "openai";
    }

    @Override
    public LlmConversationState bootstrap(String apiKey,
                                          String model,
                                          String systemPrompt,
                                          LlmConversationState conversationState) {
        String resolvedModel = resolveChatModel(model);
        Map<String, Object> policyPayload = parameterPayload(resolvedModel);
        logRequest(resolvedModel, 0, policyPayload, conversationState);
        ResponseEnvelope envelope = requestBootstrap(
                apiKey,
                resolvedModel,
                systemPrompt,
                conversationState,
                policyPayload
        );
        return envelope.conversationState();
    }

    @Override
    public LlmGenerateResult generate(String apiKey,
                                      String model,
                                      String systemPrompt,
                                      String userPrompt,
                                      LlmConversationState conversationState,
                                      LlmPromptContext promptContext) throws Exception {
        String resolvedModel = resolveChatModel(model);
        StructuredOutputException lastStructuredFailure = null;

        for (int attempt = 1; attempt <= 2; attempt += 1) {
            try {
                return structuredGenerate(
                        apiKey,
                        resolvedModel,
                        systemPrompt,
                        userPrompt,
                        conversationState,
                        promptContext,
                        attempt
                );
            } catch (StructuredOutputException e) {
                lastStructuredFailure = e;
                log.warning("openai.llm.structured_output failure model="
                        + resolvedModel
                        + " attempt="
                        + attempt
                        + " reason="
                        + defaultMessage(e.getMessage()));
            }
        }

        if (lastStructuredFailure != null) {
            return fallbackFromStructuredFailure(lastStructuredFailure);
        }
        throw new IllegalStateException("OpenAI structured output failed without fallback context");
    }

    private LlmGenerateResult structuredGenerate(String apiKey,
                                                 String model,
                                                 String systemPrompt,
                                                 String userPrompt,
                                                 LlmConversationState conversationState,
                                                 LlmPromptContext promptContext,
                                                 int attempt) {
        BeanOutputConverter<StructuredFeedbackResponse> converter = new BeanOutputConverter<>(StructuredFeedbackResponse.class);
        ResponseEnvelope envelope = requestContent(
                apiKey,
                model,
                systemPrompt,
                userPrompt,
                promptContext,
                converter.getFormat(),
                conversationState,
                attempt
        );
        try {
            StructuredFeedbackResponse response = converter.convert(envelope.content());
            Feedback feedback = Feedback.of(
                    safe(response.summary),
                    toCorrections(response.corrections),
                    toRecommendations(response.recommendations),
                    safe(response.exampleAnswer),
                    safeStringList(response.rulebookEvidence)
            );
            return new LlmGenerateResult(
                    feedback,
                    List.of(),
                    envelope.conversationState(),
                    safe(response.summary)
            );
        } catch (RuntimeException e) {
            throw new StructuredOutputException(
                    "Structured output conversion failed",
                    envelope.content(),
                    envelope.conversationState(),
                    e
            );
        }
    }

    private LlmGenerateResult fallbackFromStructuredFailure(StructuredOutputException failure) {
        List<String> reasons = List.of("structured_output_conversion_failed");
        try {
            JsonNode root = objectMapper.readTree(extractJson(failure.raw()));
            Feedback feedback = parseFallbackFeedback(root);
            return new LlmGenerateResult(feedback, reasons, failure.conversationState(), feedback.getSummary());
        } catch (Exception parseFailure) {
            log.warning(() -> "openai.llm.fallback_parse failure reason=" + defaultMessage(parseFailure.getMessage()));
            Feedback feedback = Feedback.of("", List.of(), List.of(), "", List.of());
            return new LlmGenerateResult(
                    feedback,
                    List.of("structured_output_conversion_failed", "fallback_parse_failed"),
                    failure.conversationState(),
                    ""
            );
        }
    }

    private Feedback parseFallbackFeedback(JsonNode root) {
        String summary = root.path("summary").asText("");
        String exampleAnswer = root.path("exampleAnswer").asText("");
        List<String> evidence = toStringList(root.path("rulebookEvidence"));

        JsonNode correctionsNode = root.path("corrections");
        JsonNode recommendationsNode = root.path("recommendations");

        if (correctionsNode.isObject() || recommendationsNode.isObject()) {
            Corrections corrections = new Corrections(
                    parseCorrectionDetail(correctionsNode.path("grammar")),
                    parseCorrectionDetail(correctionsNode.path("expression")),
                    parseCorrectionDetail(correctionsNode.path("logic"))
            );
            Recommendations recommendations = new Recommendations(
                    parseRecommendationDetail(recommendationsNode.path("filler")),
                    parseRecommendationDetail(recommendationsNode.path("adjective")),
                    parseRecommendationDetail(recommendationsNode.path("adverb"))
            );
            return Feedback.of(summary, corrections, recommendations, exampleAnswer, evidence);
        }

        List<String> correctionPoints = toStringList(root.path("correctionPoints"));
        List<String> recommendation = toStringList(root.path("recommendation"));
        return Feedback.of(summary, correctionPoints, recommendation, exampleAnswer, evidence);
    }

    private CorrectionDetail parseCorrectionDetail(JsonNode node) {
        if (node == null || !node.isObject()) {
            return new CorrectionDetail("", "");
        }
        return new CorrectionDetail(node.path("issue").asText(""), node.path("fix").asText(""));
    }

    private RecommendationDetail parseRecommendationDetail(JsonNode node) {
        if (node == null || !node.isObject()) {
            return new RecommendationDetail("", "");
        }
        return new RecommendationDetail(node.path("term").asText(""), node.path("usage").asText(""));
    }

    private ResponseEnvelope requestContent(String apiKey,
                                            String model,
                                            String systemPrompt,
                                            String userPrompt,
                                            LlmPromptContext promptContext,
                                            String format,
                                            LlmConversationState conversationState,
                                            int attempt) {
        Map<String, Object> policyPayload = parameterPayload(model);
        logRequest(model, attempt, policyPayload, conversationState);
        String mergedPrompt = mergePromptContext(userPrompt, promptContext);

        try {
            if (responsesEnabled) {
                return requestViaResponses(
                        apiKey,
                        model,
                        systemPrompt,
                        mergedPrompt + "\n\n" + format,
                        conversationState,
                        policyPayload,
                        attempt
                );
            }
            return requestViaLegacyChat(
                    apiKey,
                    model,
                    systemPrompt,
                    mergedPrompt + "\n\n" + format,
                    conversationState,
                    policyPayload,
                    attempt
            );
        } catch (WebClientResponseException e) {
            throw new IllegalStateException("OpenAI request failed: " + resolveErrorMessage(e.getResponseBodyAsString()), e);
        }
    }

    private ResponseEnvelope requestBootstrap(String apiKey,
                                              String model,
                                              String systemPrompt,
                                              LlmConversationState conversationState,
                                              Map<String, Object> policyPayload) {
        try {
            if (responsesEnabled) {
                return requestViaResponses(
                        apiKey,
                        model,
                        systemPrompt,
                        BOOTSTRAP_INPUT_TEXT,
                        conversationState,
                        policyPayload,
                        0
                );
            }
            return requestViaLegacyChat(
                    apiKey,
                    model,
                    systemPrompt,
                    BOOTSTRAP_INPUT_TEXT,
                    conversationState,
                    policyPayload,
                    0
            );
        } catch (WebClientResponseException e) {
            throw new IllegalStateException("OpenAI bootstrap failed: " + resolveErrorMessage(e.getResponseBodyAsString()), e);
        }
    }

    private ResponseEnvelope requestViaResponses(String apiKey,
                                                 String model,
                                                 String systemPrompt,
                                                 String prompt,
                                                 LlmConversationState conversationState,
                                                 Map<String, Object> policyPayload,
                                                 int attempt) {
        Map<String, Object> payload = new HashMap<>();
        payload.put("model", model);
        if (systemPrompt != null && !systemPrompt.isBlank()) {
            payload.put("instructions", systemPrompt);
        }
        payload.put("input", List.of(Map.of(
                "role", "user",
                "content", List.of(Map.of(
                        "type", "input_text",
                        "text", prompt
                ))
        )));
        if (policyPayload.containsKey("temperature")) {
            payload.put("temperature", policyPayload.get("temperature"));
        }
        if (conversationState != null && conversationState.hasConversationId()) {
            payload.put("conversation", conversationState.conversationId());
        }
        if (conversationState != null && conversationState.responseId() != null && !conversationState.responseId().isBlank()) {
            payload.put("previous_response_id", conversationState.responseId());
        }

        String responseBody = postJson(apiKey, "/v1/responses", payload);
        JsonNode root = parseJson(responseBody);
        String content = extractResponsesOutputText(root);
        String conversationId = extractConversationId(root, conversationState);
        String responseId = safe(root.path("id").asText(""));

        logSuccess(model, attempt, content, conversationId);
        int turns = conversationState == null ? 0 : conversationState.turnCountSinceRebase();
        LlmConversationState nextState = new LlmConversationState(conversationId, responseId, turns + 1);
        return new ResponseEnvelope(content, nextState);
    }

    private ResponseEnvelope requestViaLegacyChat(String apiKey,
                                                  String model,
                                                  String systemPrompt,
                                                  String prompt,
                                                  LlmConversationState conversationState,
                                                  Map<String, Object> policyPayload,
                                                  int attempt) {
        Map<String, Object> payload = new HashMap<>();
        payload.put("model", model);
        List<Map<String, Object>> messages = new ArrayList<>();
        if (systemPrompt != null && !systemPrompt.isBlank()) {
            messages.add(Map.of("role", "system", "content", systemPrompt));
        }
        messages.add(Map.of("role", "user", "content", prompt));
        payload.put("messages", messages);
        if (policyPayload.containsKey("temperature")) {
            payload.put("temperature", policyPayload.get("temperature"));
        }

        String responseBody = postJson(apiKey, "/v1/chat/completions", payload);
        JsonNode root = parseJson(responseBody);
        String content = safe(root.path("choices").path(0).path("message").path("content").asText(""));
        String conversationId = conversationState == null ? "" : conversationState.conversationId();
        String responseId = safe(root.path("id").asText(""));

        logSuccess(model, attempt, content, conversationId);
        int turns = conversationState == null ? 0 : conversationState.turnCountSinceRebase();
        return new ResponseEnvelope(content, new LlmConversationState(conversationId, responseId, turns + 1));
    }

    private String postJson(String apiKey, String path, Map<String, Object> payload) {
        return webClient.post()
                .uri(path)
                .headers(headers -> headers.setBearerAuth(apiKey))
                .contentType(MediaType.APPLICATION_JSON)
                .bodyValue(payload)
                .retrieve()
                .bodyToMono(String.class)
                .block();
    }

    private Map<String, Object> parameterPayload(String model) {
        Map<String, Object> policyPayload = new HashMap<>();
        policyPayload.put("model", model);
        modelParameterPolicyResolver.resolve(model).apply(policyPayload);
        return policyPayload;
    }

    private void logRequest(String model,
                            int attempt,
                            Map<String, Object> payload,
                            LlmConversationState conversationState) {
        if (!log.isLoggable(Level.FINE)) {
            return;
        }
        String optional = payload.containsKey("temperature")
                ? "temperature=" + payload.get("temperature")
                : "none";
        String conversation = conversationState == null ? "new" : defaultMessage(conversationState.conversationId());
        log.fine(() -> "openai.llm.request model="
                + model
                + " attempt="
                + attempt
                + " conversation="
                + conversation
                + " optionalParameters="
                + optional);
    }

    private void logSuccess(String model, int attempt, String responseBody, String conversationId) {
        if (!logFormatter.shouldLogSuccessAtFine() || !log.isLoggable(Level.FINE)) {
            return;
        }
        log.fine(() -> "openai.llm.response success model="
                + model
                + " attempt="
                + attempt
                + " conversation="
                + defaultMessage(conversationId)
                + " bodyPreview="
                + logFormatter.responsePreview(responseBody));
    }

    private String mergePromptContext(String userPrompt, LlmPromptContext promptContext) {
        String prompt = safe(userPrompt);
        if (promptContext == null) {
            return prompt;
        }
        String summary = safe(promptContext.summary());
        List<LlmPromptContext.TurnRecord> turns = promptContext.recentTurns();
        boolean hasTurns = turns != null && !turns.isEmpty();
        List<LlmPromptContext.RecommendationRecord> recommendationHistory = promptContext.recentRecommendations();
        boolean hasRecommendations = recommendationHistory != null && !recommendationHistory.isEmpty();
        if (summary.isBlank() && !hasTurns && !hasRecommendations) {
            return prompt;
        }

        StringBuilder builder = new StringBuilder();
        builder.append("# Previous context\n");
        if (!summary.isBlank()) {
            builder.append("Summary:\n").append(summary).append("\n\n");
        }
        if (hasTurns) {
            builder.append("Recent turns:\n");
            int index = 1;
            for (LlmPromptContext.TurnRecord turn : turns) {
                builder.append(index++)
                        .append(") Q: ")
                        .append(safe(turn.question()))
                        .append("\n")
                        .append("A: ")
                        .append(safe(turn.answer()))
                        .append("\n")
                        .append("Feedback summary: ")
                        .append(safe(turn.feedbackSummary()))
                        .append("\n");
            }
            builder.append("\n");
        }
        if (hasRecommendations) {
            builder.append("Recent recommendation terms:\n");
            int index = 1;
            for (LlmPromptContext.RecommendationRecord record : recommendationHistory) {
                builder.append(index++)
                        .append(") Filler: ")
                        .append(safe(record.fillerTerm()))
                        .append(", Adjective: ")
                        .append(safe(record.adjectiveTerm()))
                        .append(", Adverb: ")
                        .append(safe(record.adverbTerm()))
                        .append("\n");
            }
            builder.append("\n");
        }
        builder.append("# Current input\n").append(prompt);
        return builder.toString();
    }

    private String extractResponsesOutputText(JsonNode root) {
        JsonNode outputText = root.path("output_text");
        if (outputText.isTextual()) {
            return safe(outputText.asText(""));
        }
        if (outputText.isArray()) {
            List<String> lines = new ArrayList<>();
            for (JsonNode node : outputText) {
                String line = safe(node.asText(""));
                if (!line.isBlank()) {
                    lines.add(line);
                }
            }
            if (!lines.isEmpty()) {
                return String.join("\n", lines);
            }
        }

        List<String> fragments = new ArrayList<>();
        JsonNode output = root.path("output");
        if (output.isArray()) {
            for (JsonNode item : output) {
                JsonNode content = item.path("content");
                if (!content.isArray()) {
                    continue;
                }
                for (JsonNode part : content) {
                    String text = safe(part.path("text").asText(""));
                    if (!text.isBlank()) {
                        fragments.add(text);
                    }
                }
            }
        }
        return String.join("\n", fragments);
    }

    private String extractConversationId(JsonNode root, LlmConversationState fallback) {
        JsonNode conversation = root.path("conversation");
        if (conversation.isTextual()) {
            return safe(conversation.asText(""));
        }
        if (conversation.isObject()) {
            String fromObject = safe(conversation.path("id").asText(""));
            if (!fromObject.isBlank()) {
                return fromObject;
            }
        }
        String conversationId = safe(root.path("conversation_id").asText(""));
        if (!conversationId.isBlank()) {
            return conversationId;
        }
        if (fallback == null) {
            return "";
        }
        return safe(fallback.conversationId());
    }

    private JsonNode parseJson(String body) {
        try {
            return objectMapper.readTree(body == null ? "{}" : body);
        } catch (Exception e) {
            throw new IllegalStateException("OpenAI response parse failed", e);
        }
    }

    private String resolveErrorMessage(String body) {
        try {
            JsonNode root = objectMapper.readTree(body == null ? "{}" : body);
            String message = safe(root.path("error").path("message").asText(""));
            if (!message.isBlank()) {
                return message;
            }
        } catch (Exception ignored) {
            // ignored: return raw body below
        }
        return defaultMessage(body);
    }

    private Corrections toCorrections(StructuredFeedbackResponse.CorrectionsNode node) {
        StructuredFeedbackResponse.CorrectionsNode safe = node == null ? new StructuredFeedbackResponse.CorrectionsNode() : node;
        return new Corrections(
                new CorrectionDetail(safeGrammarIssue(safe), safeGrammarFix(safe)),
                new CorrectionDetail(safeExpressionIssue(safe), safeExpressionFix(safe)),
                new CorrectionDetail(safeLogicIssue(safe), safeLogicFix(safe))
        );
    }

    private Recommendations toRecommendations(StructuredFeedbackResponse.RecommendationsNode node) {
        StructuredFeedbackResponse.RecommendationsNode safe = node == null ? new StructuredFeedbackResponse.RecommendationsNode() : node;
        return new Recommendations(
                new RecommendationDetail(safeFillerTerm(safe), safeFillerUsage(safe)),
                new RecommendationDetail(safeAdjectiveTerm(safe), safeAdjectiveUsage(safe)),
                new RecommendationDetail(safeAdverbTerm(safe), safeAdverbUsage(safe))
        );
    }

    private String safeGrammarIssue(StructuredFeedbackResponse.CorrectionsNode node) {
        return safe(node.grammar == null ? null : node.grammar.issue);
    }

    private String safeGrammarFix(StructuredFeedbackResponse.CorrectionsNode node) {
        return safe(node.grammar == null ? null : node.grammar.fix);
    }

    private String safeExpressionIssue(StructuredFeedbackResponse.CorrectionsNode node) {
        return safe(node.expression == null ? null : node.expression.issue);
    }

    private String safeExpressionFix(StructuredFeedbackResponse.CorrectionsNode node) {
        return safe(node.expression == null ? null : node.expression.fix);
    }

    private String safeLogicIssue(StructuredFeedbackResponse.CorrectionsNode node) {
        return safe(node.logic == null ? null : node.logic.issue);
    }

    private String safeLogicFix(StructuredFeedbackResponse.CorrectionsNode node) {
        return safe(node.logic == null ? null : node.logic.fix);
    }

    private String safeFillerTerm(StructuredFeedbackResponse.RecommendationsNode node) {
        return safe(node.filler == null ? null : node.filler.term);
    }

    private String safeFillerUsage(StructuredFeedbackResponse.RecommendationsNode node) {
        return safe(node.filler == null ? null : node.filler.usage);
    }

    private String safeAdjectiveTerm(StructuredFeedbackResponse.RecommendationsNode node) {
        return safe(node.adjective == null ? null : node.adjective.term);
    }

    private String safeAdjectiveUsage(StructuredFeedbackResponse.RecommendationsNode node) {
        return safe(node.adjective == null ? null : node.adjective.usage);
    }

    private String safeAdverbTerm(StructuredFeedbackResponse.RecommendationsNode node) {
        return safe(node.adverb == null ? null : node.adverb.term);
    }

    private String safeAdverbUsage(StructuredFeedbackResponse.RecommendationsNode node) {
        return safe(node.adverb == null ? null : node.adverb.usage);
    }

    private String safe(String value) {
        return value == null ? "" : value.trim();
    }

    private List<String> safeStringList(List<String> values) {
        if (values == null || values.isEmpty()) {
            return List.of();
        }
        return values.stream()
                .filter(value -> value != null && !value.isBlank())
                .map(String::trim)
                .toList();
    }

    private List<String> toStringList(JsonNode arrayNode) {
        List<String> values = new ArrayList<>();
        if (arrayNode == null || !arrayNode.isArray()) {
            return values;
        }
        for (JsonNode node : arrayNode) {
            values.add(node.asText(""));
        }
        return values;
    }

    private String extractJson(String raw) {
        if (raw == null || raw.isBlank()) {
            return "{}";
        }
        int start = raw.indexOf('{');
        int end = raw.lastIndexOf('}');
        if (start >= 0 && end > start) {
            return raw.substring(start, end + 1);
        }
        return raw;
    }

    private String resolveChatModel(String model) {
        String candidate = model == null ? "" : model.trim();
        if (candidate.isBlank()) {
            return DEFAULT_CHAT_MODEL;
        }
        String lowered = candidate.toLowerCase(Locale.ROOT);
        if (lowered.contains("transcribe")) {
            return DEFAULT_CHAT_MODEL;
        }
        return candidate;
    }

    private String defaultMessage(String message) {
        return message == null || message.isBlank() ? "unknown" : message;
    }

    private record ResponseEnvelope(String content, LlmConversationState conversationState) {
    }

    private static final class StructuredOutputException extends RuntimeException {
        private final String raw;
        private final LlmConversationState conversationState;

        private StructuredOutputException(String message,
                                          String raw,
                                          LlmConversationState conversationState,
                                          Throwable cause) {
            super(message, cause);
            this.raw = raw == null ? "" : raw;
            this.conversationState = conversationState == null ? LlmConversationState.empty() : conversationState;
        }

        private String raw() {
            return raw;
        }

        private LlmConversationState conversationState() {
            return conversationState;
        }
    }

    public static final class StructuredFeedbackResponse {
        public String summary;
        public CorrectionsNode corrections;
        public RecommendationsNode recommendations;
        public String exampleAnswer;
        public List<String> rulebookEvidence;

        public static final class CorrectionsNode {
            public CorrectionNode grammar;
            public CorrectionNode expression;
            public CorrectionNode logic;
        }

        public static final class CorrectionNode {
            public String issue;
            public String fix;
        }

        public static final class RecommendationsNode {
            public RecommendationNode filler;
            public RecommendationNode adjective;
            public RecommendationNode adverb;
        }

        public static final class RecommendationNode {
            public String term;
            public String usage;
        }
    }
}
