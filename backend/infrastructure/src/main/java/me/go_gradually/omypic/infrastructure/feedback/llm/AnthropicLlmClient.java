package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.anthropic.core.JsonValue;
import com.anthropic.errors.AnthropicIoException;
import com.anthropic.errors.AnthropicServiceException;
import com.anthropic.errors.NotFoundException;
import com.anthropic.errors.PermissionDeniedException;
import com.anthropic.errors.RateLimitException;
import com.anthropic.errors.UnauthorizedException;
import com.anthropic.models.beta.AnthropicBeta;
import com.anthropic.models.beta.messages.BetaCacheControlEphemeral;
import com.anthropic.models.beta.messages.BetaFallbacksParam;
import com.anthropic.models.beta.messages.BetaJsonOutputFormat;
import com.anthropic.models.beta.messages.BetaMessage;
import com.anthropic.models.beta.messages.BetaOutputConfig;
import com.anthropic.models.beta.messages.BetaRefusalStopDetails;
import com.anthropic.models.beta.messages.BetaStopReason;
import com.anthropic.models.beta.messages.BetaTextBlockParam;
import com.anthropic.models.beta.messages.MessageCreateParams;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.policy.FeedbackModelPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.session.LlmConversationState;
import me.go_gradually.omypic.domain.session.LlmPromptContext;
import me.go_gradually.omypic.infrastructure.shared.anthropic.AnthropicClientFactory;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.stereotype.Component;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.logging.Level;
import java.util.logging.Logger;
import java.util.stream.Collectors;

/**
 * Feedback generation on the Claude Messages API.
 *
 * Unlike the OpenAI Responses client, Claude keeps no conversation on the server:
 * each call sends the full system prompt, and session memory travels in the user prompt.
 * The response shape is enforced with structured outputs instead of prompt-only instructions.
 */
@Component
public class AnthropicLlmClient implements LlmClient {
    private static final Logger log = Logger.getLogger(AnthropicLlmClient.class.getName());
    private static final String STRUCTURED_OUTPUT_FAILED = "structured_output_conversion_failed";
    /**
     * Models whose safety classifiers can decline a request and that support routing the
     * request to a fallback model on the server. Claude Haiku 5.5 has no server-side fallback.
     */
    private static final Set<String> SERVER_FALLBACK_MODELS = Set.of(
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-fable-5-2"
    );

    private final ObjectMapper objectMapper = new ObjectMapper();
    private final AnthropicClientFactory clientFactory;
    private final long maxTokens;
    private final BetaOutputConfig outputConfig;

    public AnthropicLlmClient(AppProperties properties, AnthropicClientFactory clientFactory) {
        AppProperties.Anthropic anthropic = properties.getIntegrations().getAnthropic();
        this.clientFactory = clientFactory;
        this.maxTokens = Math.max(1024L, anthropic.getMaxTokens());
        this.outputConfig = BetaOutputConfig.builder()
                .effort(BetaOutputConfig.Effort.of(normalizeEffort(anthropic.getEffort())))
                .format(feedbackFormat())
                .build();
    }

    @Override
    public String provider() {
        return FeedbackModelPolicy.ANTHROPIC_PROVIDER;
    }

    @Override
    public boolean isStateless() {
        return true;
    }

    @Override
    public LlmConversationState bootstrap(String apiKey,
                                          String model,
                                          String systemPrompt,
                                          LlmConversationState conversationState) {
        // Nothing to prime: the system prompt is sent with every generate call.
        return LlmConversationState.empty();
    }

    @Override
    public LlmGenerateResult generate(String apiKey,
                                      String model,
                                      String systemPrompt,
                                      String userPrompt,
                                      LlmConversationState conversationState,
                                      LlmPromptContext promptContext) {
        String resolvedModel = resolveModel(model);
        String prompt = LlmPromptContextRenderer.merge(userPrompt, promptContext);
        BetaMessage message = send(apiKey, buildParams(resolvedModel, systemPrompt, prompt));
        logUsage(resolvedModel, message);
        rejectRefusal(message);
        return toResult(resolvedModel, message);
    }

    private MessageCreateParams buildParams(String model, String systemPrompt, String prompt) {
        MessageCreateParams.Builder builder = MessageCreateParams.builder()
                .model(model)
                .maxTokens(maxTokens)
                .outputConfig(outputConfig)
                .addUserMessage(prompt);
        if (systemPrompt != null && !systemPrompt.isBlank()) {
            builder.systemOfBetaTextBlockParams(List.of(cachedSystemBlock(systemPrompt)));
        }
        enableRefusalFallback(builder, model);
        return builder.build();
    }

    private void enableRefusalFallback(MessageCreateParams.Builder builder, String model) {
        if (SERVER_FALLBACK_MODELS.contains(model)) {
            builder.addBeta(AnthropicBeta.SERVER_SIDE_FALLBACK_2026_07_01)
                    .fallbacks(BetaFallbacksParam.ofDefault());
        }
    }

    private BetaTextBlockParam cachedSystemBlock(String systemPrompt) {
        // The coaching prompt repeats across the calls of one turn; prefixes below the
        // model's minimum cacheable size are simply not cached.
        return BetaTextBlockParam.builder()
                .text(systemPrompt)
                .cacheControl(BetaCacheControlEphemeral.builder().build())
                .build();
    }

    private BetaMessage send(String apiKey, MessageCreateParams params) {
        try {
            return clientFactory.forApiKey(apiKey).beta().messages().create(params);
        } catch (AnthropicServiceException e) {
            throw failure(describe(e), e);
        } catch (AnthropicIoException e) {
            throw failure("could not reach the Anthropic API", e);
        }
    }

    private String describe(AnthropicServiceException error) {
        if (error instanceof UnauthorizedException) {
            return "the Anthropic API key was rejected";
        }
        if (error instanceof PermissionDeniedException) {
            return "this API key cannot use the selected Claude model";
        }
        if (error instanceof NotFoundException) {
            return "the selected Claude model was not found";
        }
        if (error instanceof RateLimitException) {
            return "rate limit reached, try again shortly";
        }
        return "HTTP " + error.statusCode() + " " + defaultMessage(error.getMessage());
    }

    private IllegalStateException failure(String reason, Exception cause) {
        return new IllegalStateException("Claude request failed: " + reason, cause);
    }

    private void rejectRefusal(BetaMessage message) {
        boolean refused = message.stopReason().map(BetaStopReason.REFUSAL::equals).orElse(false);
        if (!refused) {
            return;
        }
        String category = message.stopDetails()
                .flatMap(BetaRefusalStopDetails::category)
                .map(BetaRefusalStopDetails.Category::asString)
                .orElse("unspecified");
        throw new IllegalStateException("Claude declined to answer this turn (category: " + category + ")");
    }

    private LlmGenerateResult toResult(String model, BetaMessage message) {
        String content = textOf(message);
        try {
            JsonNode root = objectMapper.readTree(FeedbackJsonParser.extractJson(content));
            Feedback feedback = FeedbackJsonParser.parse(root);
            return new LlmGenerateResult(feedback, List.of(), LlmConversationState.empty(), feedback.getSummary());
        } catch (Exception parseFailure) {
            return unparsableResult(model, message, parseFailure);
        }
    }

    private LlmGenerateResult unparsableResult(String model, BetaMessage message, Exception parseFailure) {
        // Structured outputs guarantee the schema, so this only happens when the reply was cut off.
        log.warning(() -> "anthropic.llm.structured_output failure model=" + model
                + " stopReason=" + stopReasonOf(message)
                + " reason=" + defaultMessage(parseFailure.getMessage()));
        Feedback empty = Feedback.of("", List.of(), List.of(), "", List.of());
        return new LlmGenerateResult(empty, List.of(STRUCTURED_OUTPUT_FAILED), LlmConversationState.empty(), "");
    }

    private String textOf(BetaMessage message) {
        return message.content().stream()
                .flatMap(block -> block.text().stream())
                .map(text -> text.text())
                .collect(Collectors.joining());
    }

    private void logUsage(String model, BetaMessage message) {
        if (!log.isLoggable(Level.FINE)) {
            return;
        }
        log.fine(() -> "anthropic.llm.response model=" + model
                + " servedBy=" + message.model().asString()
                + " stopReason=" + stopReasonOf(message)
                + " inputTokens=" + message.usage().inputTokens()
                + " outputTokens=" + message.usage().outputTokens()
                + " cacheReadTokens=" + message.usage().cacheReadInputTokens().orElse(0L));
    }

    private String stopReasonOf(BetaMessage message) {
        return message.stopReason().map(BetaStopReason::asString).orElse("unknown");
    }

    private String resolveModel(String model) {
        String candidate = model == null ? "" : model.trim().toLowerCase(Locale.ROOT);
        if (candidate.isBlank()) {
            return FeedbackModelPolicy.defaultModel(FeedbackModelPolicy.ANTHROPIC_PROVIDER);
        }
        return candidate;
    }

    private String normalizeEffort(String effort) {
        String candidate = effort == null ? "" : effort.trim().toLowerCase(Locale.ROOT);
        return Set.of("low", "medium", "high", "xhigh", "max").contains(candidate) ? candidate : "low";
    }

    private String defaultMessage(String message) {
        return message == null || message.isBlank() ? "unknown" : message;
    }

    private static BetaJsonOutputFormat feedbackFormat() {
        BetaJsonOutputFormat.Schema.Builder schema = BetaJsonOutputFormat.Schema.builder();
        feedbackSchema().forEach((key, value) -> schema.putAdditionalProperty(key, JsonValue.from(value)));
        return BetaJsonOutputFormat.builder().schema(schema.build()).build();
    }

    private static Map<String, Object> feedbackSchema() {
        Map<String, Object> properties = new LinkedHashMap<>();
        properties.put("summary", Map.of("type", "string"));
        properties.put("corrections", pairGroup(List.of("grammar", "expression", "logic"), "issue", "fix"));
        properties.put("recommendations", pairGroup(List.of("filler", "adjective", "adverb"), "term", "usage"));
        properties.put("exampleAnswer", Map.of("type", "string"));
        properties.put("rulebookEvidence", Map.of("type", "array", "items", Map.of("type", "string")));
        return objectSchema(properties);
    }

    private static Map<String, Object> pairGroup(List<String> names, String first, String second) {
        Map<String, Object> pair = new LinkedHashMap<>();
        pair.put(first, Map.of("type", "string"));
        pair.put(second, Map.of("type", "string"));
        Map<String, Object> properties = new LinkedHashMap<>();
        names.forEach(name -> properties.put(name, objectSchema(pair)));
        return objectSchema(properties);
    }

    private static Map<String, Object> objectSchema(Map<String, Object> properties) {
        Map<String, Object> schema = new LinkedHashMap<>();
        schema.put("type", "object");
        schema.put("properties", properties);
        schema.put("required", List.copyOf(properties.keySet()));
        schema.put("additionalProperties", false);
        return schema;
    }
}
