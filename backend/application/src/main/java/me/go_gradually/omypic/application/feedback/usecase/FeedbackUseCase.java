package me.go_gradually.omypic.application.feedback.usecase;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.FeedbackConstraints;
import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;

import java.time.Duration;
import java.time.Instant;
import java.util.*;
import java.util.stream.Collectors;

public class FeedbackUseCase {
    private final Map<String, LlmClient> clients;
    private final RulebookUseCase rulebookUseCase;
    private final FeedbackPolicy feedbackPolicy;
    private final ObjectMapper objectMapper = new ObjectMapper();
    private final MetricsPort metrics;
    private final SessionStorePort sessionStore;
    private final WrongNoteUseCase wrongNoteUseCase;

    public FeedbackUseCase(List<LlmClient> clientList,
                           RulebookUseCase rulebookUseCase,
                           FeedbackPolicy feedbackPolicy,
                           MetricsPort metrics,
                           SessionStorePort sessionStore,
                           WrongNoteUseCase wrongNoteUseCase) {
        this.clients = clientList.stream().collect(Collectors.toMap(LlmClient::provider, c -> c));
        this.rulebookUseCase = rulebookUseCase;
        this.feedbackPolicy = feedbackPolicy;
        this.metrics = metrics;
        this.sessionStore = sessionStore;
        this.wrongNoteUseCase = wrongNoteUseCase;
    }

    public FeedbackResult generateFeedback(String apiKey, FeedbackCommand command) {
        SessionState state = sessionStore.getOrCreate(SessionId.of(command.getSessionId()));
        FeedbackLanguage language = FeedbackLanguage.of(command.getFeedbackLanguage());
        state.setFeedbackLanguage(language);
        if (!state.shouldGenerateFeedback()) {
            return FeedbackResult.skipped();
        }
        String inputText = state.resolveFeedbackInputText(command.getText());
        List<RulebookContext> contexts = rulebookUseCase.searchContexts(inputText);
        return FeedbackResult.generated(generateFeedbackInternal(apiKey, command, language, inputText, contexts));
    }

    public FeedbackResult generateMockExamFinalFeedback(String apiKey, FeedbackCommand command) {
        SessionState state = sessionStore.getOrCreate(SessionId.of(command.getSessionId()));
        FeedbackLanguage language = FeedbackLanguage.of(command.getFeedbackLanguage());
        state.setFeedbackLanguage(language);
        if (!state.isMockExamCompleted()) {
            throw new IllegalStateException("Mock exam is not completed");
        }
        if (state.isMockFinalFeedbackGenerated()) {
            throw new IllegalStateException("Mock final feedback already generated");
        }
        String inputText = state.buildMockFinalFeedbackInput();
        List<RulebookContext> contexts = rulebookUseCase.searchContexts(inputText);
        FeedbackResult result = FeedbackResult.generated(generateFeedbackInternal(apiKey, command, language, inputText, contexts));
        state.markMockFinalFeedbackGenerated();
        return result;
    }

    public Feedback generateFeedbackForTurn(String apiKey,
                                            FeedbackCommand command,
                                            String questionText,
                                            QuestionGroup questionGroup,
                                            String answerText,
                                            int maxRulebookDocuments) {
        FeedbackLanguage language = FeedbackLanguage.of(command.getFeedbackLanguage());
        String query = buildTurnQuery(questionText, answerText);
        List<RulebookContext> contexts = rulebookUseCase.searchContextsForTurn(questionGroup, query, maxRulebookDocuments);
        String inputText = buildTurnInput(questionText, answerText, language.value());
        return generateFeedbackInternal(apiKey, command, language, inputText, contexts);
    }

    private Feedback generateFeedbackInternal(String apiKey,
                                              FeedbackCommand command,
                                              FeedbackLanguage language,
                                              String inputText,
                                              List<RulebookContext> contexts) {
        String text = inputText == null ? "" : inputText;
        List<RulebookContext> safeContexts = contexts == null ? List.of() : contexts;
        Instant start = Instant.now();
        String systemPrompt = buildSystemPrompt(language.value(), safeContexts);
        String userPrompt = buildUserPrompt(text, language.value());

        String provider = command.getProvider() == null ? "" : command.getProvider().toLowerCase(Locale.ROOT);
        LlmClient client = Optional.ofNullable(clients.get(provider))
                .orElseThrow(() -> new IllegalArgumentException("Unknown provider"));

        try {
            String raw = client.generate(apiKey, command.getModel(), systemPrompt, userPrompt);
            Feedback feedback = parseFeedback(raw);
            FeedbackConstraints constraints = new FeedbackConstraints(
                    feedbackPolicy.getSummaryMaxChars(),
                    feedbackPolicy.getExampleMinRatio(),
                    feedbackPolicy.getExampleMaxRatio()
            );
            feedback = feedback.normalized(constraints, text, language, safeContexts);
            metrics.recordFeedbackLatency(Duration.between(start, Instant.now()));
            wrongNoteUseCase.addFeedback(feedback);
            return feedback;
        } catch (Exception e) {
            metrics.incrementFeedbackError();
            throw new IllegalStateException("LLM feedback failed", e);
        }
    }

    private String buildSystemPrompt(String language, List<RulebookContext> contexts) {
        String lang = "en".equalsIgnoreCase(language) ? "English" : "Korean";
        StringBuilder sb = new StringBuilder();
        sb.append("You are a speaking coach. Output strictly valid JSON with keys: summary, correctionPoints, exampleAnswer, rulebookEvidence. ");
        sb.append("summary: 1-2 sentences. correctionPoints: array of 3 items, include at least 2 categories among Grammar, Expression, Logic. ");
        sb.append("exampleAnswer length 0.8-1.2x of user answer. rulebookEvidence: array (empty if none). ");
        sb.append("Write all text in ").append(lang).append(".");
        if (!contexts.isEmpty()) {
            sb.append(" Use the following rulebook excerpts when relevant:\n");
            for (RulebookContext ctx : contexts) {
                sb.append("- [").append(ctx.filename()).append("] ").append(ctx.text()).append("\n");
            }
        }
        return sb.toString();
    }

    private String buildUserPrompt(String text, String language) {
        String label = "en".equalsIgnoreCase(language) ? "User answer" : "사용자 답변";
        return label + ":\n" + text;
    }

    private String buildTurnQuery(String questionText, String answerText) {
        String q = questionText == null ? "" : questionText.trim();
        String a = answerText == null ? "" : answerText.trim();
        if (!q.isBlank() && !a.isBlank()) {
            return q + "\n" + a;
        }
        if (!a.isBlank()) {
            return a;
        }
        return q;
    }

    private String buildTurnInput(String questionText, String answerText, String language) {
        boolean english = "en".equalsIgnoreCase(language);
        String questionLabel = english ? "Question" : "질문";
        String answerLabel = english ? "Answer" : "답변";
        String safeQuestion = questionText == null ? "" : questionText.trim();
        String safeAnswer = answerText == null ? "" : answerText.trim();
        StringBuilder sb = new StringBuilder();
        if (!safeQuestion.isBlank()) {
            sb.append(questionLabel).append(":\n").append(safeQuestion).append('\n');
        }
        sb.append(answerLabel).append(":\n").append(safeAnswer);
        return sb.toString().trim();
    }

    private Feedback parseFeedback(String raw) throws Exception {
        String json = extractJson(raw);
        JsonNode root = objectMapper.readTree(json);
        String summary = root.path("summary").asText("");
        List<String> points = new ArrayList<>();
        for (JsonNode node : root.path("correctionPoints")) {
            points.add(node.asText());
        }
        String exampleAnswer = root.path("exampleAnswer").asText("");
        List<String> evidence = new ArrayList<>();
        for (JsonNode node : root.path("rulebookEvidence")) {
            evidence.add(node.asText());
        }
        return Feedback.of(summary, points, exampleAnswer, evidence);
    }

    private String extractJson(String raw) {
        int start = raw.indexOf('{');
        int end = raw.lastIndexOf('}');
        if (start >= 0 && end > start) {
            return raw.substring(start, end + 1);
        }
        return raw;
    }
}
