package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.domain.feedback.CorrectionDetail;
import me.go_gradually.omypic.domain.feedback.Corrections;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.RecommendationDetail;
import me.go_gradually.omypic.domain.feedback.Recommendations;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.ai.chat.messages.SystemMessage;
import org.springframework.ai.chat.messages.UserMessage;
import org.springframework.ai.chat.model.ChatResponse;
import org.springframework.ai.chat.prompt.Prompt;
import org.springframework.ai.converter.BeanOutputConverter;
import org.springframework.ai.openai.OpenAiChatModel;
import org.springframework.ai.openai.OpenAiChatOptions;
import org.springframework.ai.openai.api.OpenAiApi;
import org.springframework.stereotype.Component;

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
    private static final Logger log = Logger.getLogger(OpenAiLlmClient.class.getName());

    private final ObjectMapper objectMapper = new ObjectMapper();
    private final OpenAiModelParameterPolicyResolver modelParameterPolicyResolver = new OpenAiModelParameterPolicyResolver();
    private final OpenAiLlmLogFormatter logFormatter;
    private final String baseUrl;

    public OpenAiLlmClient(AppProperties properties) {
        this.logFormatter = OpenAiLlmLogFormatter.from(properties);
        this.baseUrl = properties.getIntegrations().getOpenai().getBaseUrl();
    }

    @Override
    public String provider() {
        return "openai";
    }

    @Override
    public LlmGenerateResult generate(String apiKey, String model, String systemPrompt, String userPrompt) throws Exception {
        String resolvedModel = resolveChatModel(model);
        StructuredOutputException lastStructuredFailure = null;

        for (int attempt = 1; attempt <= 2; attempt += 1) {
            try {
                return structuredGenerate(apiKey, resolvedModel, systemPrompt, userPrompt, attempt);
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
                                                 int attempt) {
        BeanOutputConverter<StructuredFeedbackResponse> converter = new BeanOutputConverter<>(StructuredFeedbackResponse.class);
        String content = requestContent(apiKey, model, systemPrompt, userPrompt, converter.getFormat(), attempt);
        try {
            StructuredFeedbackResponse response = converter.convert(content);
            Feedback feedback = Feedback.of(
                    safe(response.summary),
                    toCorrections(response.corrections),
                    toRecommendations(response.recommendations),
                    safe(response.exampleAnswer),
                    safeStringList(response.rulebookEvidence)
            );
            return new LlmGenerateResult(feedback, List.of());
        } catch (RuntimeException e) {
            throw new StructuredOutputException("Structured output conversion failed", content, e);
        }
    }

    private LlmGenerateResult fallbackFromStructuredFailure(StructuredOutputException failure) {
        List<String> reasons = List.of("structured_output_conversion_failed");
        try {
            JsonNode root = objectMapper.readTree(extractJson(failure.raw()));
            Feedback feedback = parseFallbackFeedback(root);
            return new LlmGenerateResult(feedback, reasons);
        } catch (Exception parseFailure) {
            log.warning(() -> "openai.llm.fallback_parse failure reason=" + defaultMessage(parseFailure.getMessage()));
            Feedback feedback = Feedback.of("", List.of(), List.of(), "", List.of());
            return new LlmGenerateResult(feedback, List.of("structured_output_conversion_failed", "fallback_parse_failed"));
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

    private String requestContent(String apiKey,
                                  String model,
                                  String systemPrompt,
                                  String userPrompt,
                                  String format,
                                  int attempt) {
        OpenAiChatOptions options = buildOptions(model);
        logRequest(model, attempt, options);
        Prompt prompt = new Prompt(
                List.of(
                        new SystemMessage(systemPrompt),
                        new UserMessage(userPrompt + "\n\n" + format)
                ),
                options
        );
        ChatResponse response = chatModel(apiKey, options).call(prompt);
        String content = response == null || response.getResult() == null || response.getResult().getOutput() == null
                ? ""
                : response.getResult().getOutput().getText();
        logSuccess(model, attempt, content);
        return content == null ? "" : content;
    }

    private OpenAiChatModel chatModel(String apiKey, OpenAiChatOptions options) {
        OpenAiApi openAiApi = OpenAiApi.builder()
                .baseUrl(baseUrl)
                .apiKey(apiKey)
                .build();
        return OpenAiChatModel.builder()
                .openAiApi(openAiApi)
                .defaultOptions(options)
                .build();
    }

    private OpenAiChatOptions buildOptions(String model) {
        Map<String, Object> policyPayload = new HashMap<>();
        policyPayload.put("model", model);
        modelParameterPolicyResolver.resolve(model).apply(policyPayload);

        OpenAiChatOptions.Builder builder = OpenAiChatOptions.builder().model(model);
        Object temperature = policyPayload.get("temperature");
        if (temperature instanceof Number number) {
            builder.temperature(number.doubleValue());
        }
        return builder.build();
    }

    private void logRequest(String model, int attempt, OpenAiChatOptions options) {
        if (!log.isLoggable(Level.FINE)) {
            return;
        }
        String optional = options.getTemperature() == null ? "none" : "temperature=" + options.getTemperature();
        log.fine(() -> "openai.llm.request model=" + model + " attempt=" + attempt + " optionalParameters=" + optional);
    }

    private void logSuccess(String model, int attempt, String responseBody) {
        if (!logFormatter.shouldLogSuccessAtFine() || !log.isLoggable(Level.FINE)) {
            return;
        }
        log.fine(() -> "openai.llm.response success model="
                + model
                + " attempt="
                + attempt
                + " bodyPreview="
                + logFormatter.responsePreview(responseBody));
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

    private static final class StructuredOutputException extends RuntimeException {
        private final String raw;

        private StructuredOutputException(String message, String raw, Throwable cause) {
            super(message, cause);
            this.raw = raw == null ? "" : raw;
        }

        private String raw() {
            return raw;
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
