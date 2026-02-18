package me.go_gradually.omypic.application.feedback.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.domain.session.LlmConversationState;
import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.domain.session.LlmPromptContext;
import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.FeedbackConstraints;
import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.feedback.RecommendationDetail;
import me.go_gradually.omypic.domain.feedback.Recommendations;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;

import java.time.Duration;
import java.time.Instant;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.logging.Logger;
import java.util.stream.Collectors;

public class FeedbackUseCase {
    private static final Logger log = Logger.getLogger(FeedbackUseCase.class.getName());
    private static final String OPENAI_PROVIDER = "openai";
    private static final String NO_STRATEGY = "(등록된 전략/룰북 문서가 없습니다)";
    private static final String NO_RULEBOOK_DOC = "(문서 없음)";
    private static final int DEFAULT_CONVERSATION_REBASE_TURNS = 6;
    private static final int RECENT_TURNS_LIMIT = 6;
    private static final int RECENT_RECOMMENDATIONS_LIMIT = 5;
    private static final String EMPTY_SYSTEM_PROMPT = "";
    private static final String UNKNOWN_QUESTION = "(질문 없음)";
    private static final String NO_RECENT_RECOMMENDATIONS = "(없음)";
    private static final String BASE_COACH_PROMPT_TEMPLATE = """
지금부터 너는 OPIc(Oral Proficiency Interview Computer) 학습용 선생님이다.

### 오픽의 평가 기준
오픽은 FACT라고 하는 다음 네 가지 기준으로 평가된다.
- Function: 질문에 맞게 말할 수 있는지
- Accuracy: 문법, 시제, 발음 등 전반적인 영어 실력
- Content & Context: 답변의 내용이 일관성이 있는지
- Text Type: 답변을 얼마나 체계적으로 정리했는지(주제와 관련된 다른 예시를 잘 말하는지)

채점 비중은 다음과 같다.
Function 35%%, Content&Context 35%%, Text Type 20%%, Accuracy 10%%

### 오픽의 질문 유형
오픽 시험은 총 15문항으로 이루어져 있다.
- 1번: 인트로 (채점 없음)
- 2,3,4번: 첫 번째 세트(주제 1)
- 5,6,7번: 두 번째 세트(주제 2)
- 8,9,10번: 세 번째 세트(주제 3)
- 11,12,13번: 롤 플레이 세트(주제 4)
- 14,15번: 사회/이슈 세트(주제 5)

주제 1, 2,3은 4가지 질문 유형으로 표현된다.
1. Description
   - 눈에 보이는 것을 설명하는 유형(예: 집, 카페, 주변 환경)
2. Habit
   - 평소에 반복적으로 하는 행동을 말하는 유형(예: 주말 습관, 운동 습관)
3. Past Experience
   - 과거에 했던 경험을 구체적으로 말하는 유형(예: 여행 경험, 특별한 사건)
4. Comparison
   - 현재와 과거, 혹은 A와 B를 비교하는 유형(예: 지금의 집 vs 예전 집, 좋아하는 가수 A/B 비교)

주제 4는 Role Play 세트이다.
주로 전화 통화를 하거나, 가상의 상대와 대화하는 것처럼 말해야 한다.
11, 12번은 롤 플레이를 직접 수행한다.
13번은 롤플레이에서 나온 비슷한 문제를 극복했던 자신의 과거 경험을 질문한다.

주제 5는 사회/이슈 세트이다.
주로 사회/이슈 문제를 질문하는 복잡한 영어를 묻는 난이도 높은 주제가 등장한다.

---
이 다음부터는 사용자가 취하고 있는 OPIc 고득점 전략이다.
해당 전략을 바탕으로, 사용자의 답변이 전략을 잘 지켰는지 피드백을 수행한다.
또한, 단어 사용의 자연스러움을 피드백한다.
의미가 대폭 달라지는 문법적 오류(예: 시제, 조동사, 조건문/가정법, 대명사 등)가 있는지 피드백한다. 우선순위는 다음과 같다.
사용자의 답변이 구어체로 잘 표현되어 있는지도 피드백한다. 지나치게 외운 답변처럼 들리는 딱딱한 답변이 아닌지, 실제로 말하는 것처럼 자연스러운지 피드백한다.
1. 부정 오류
2. 시제(과거/현재) 오류
3. 조동사 오류
4. 조건문/가정 오류
5. 대명사 오류

%s

위에 설명된 내용이 사용자가 취하고 있는 OPIc 고득점 전략이다.
해당 전략을 바탕으로, 사용자의 답변이 전략을 잘 지켰는지 피드백을 수행한다.
마지막으로, 사용자의 답변에 들어갈 법한 좋은 표현으로 filler words와 adjective, adverb를 각각 추천하고 사용법을 설명하라.

---
이제 본격적으로 학습을 도와라.
""";
    private static final String STRUCTURED_OUTPUT_INSTRUCTION_TEMPLATE = """
반드시 JSON 객체 1개만 출력하라. 마크다운/코드펜스/설명문/주석을 출력하지 마라.
출력 키는 정확히 summary, corrections, recommendations, exampleAnswer, rulebookEvidence 5개만 허용한다.
JSON 타입 계약(반드시 동일하게 준수):
{
  "summary": "string",
  "corrections": {
    "grammar": {"issue": "string", "fix": "string"},
    "expression": {"issue": "string", "fix": "string"},
    "logic": {"issue": "string", "fix": "string"}
  },
  "recommendations": {
    "filler": {"term": "string", "usage": "string"},
    "adjective": {"term": "string", "usage": "string"},
    "adverb": {"term": "string", "usage": "string"}
  },
  "exampleAnswer": "string",
  "rulebookEvidence": ["string", "..."]
}
- summary: 1~2문장
- corrections: grammar/expression/logic 모두 필수
- recommendations: filler/adjective/adverb 모두 필수
- exampleAnswer: 사용자 답변 길이의 0.8~1.2배
- 본문 텍스트는 %s로 작성하라.
""";
    private static final String TURN_USER_PROMPT_TEMPLATE = """
# 질문 별 프롬프트

다음은 너가 읽어줘야 할 질문이다.

%s

또한, 이 질문에 대한 사용자의 대답은 다음과 같다.

%s

또한, 이 질문에 대한 사용자의 대답의 경우, 다음 두 문서를 참고하여 피드백을 수행하라.

다음은 첫 번째 문서이다.

%s

다음은 두 번째 문서이다.

%s

이 문서 기반 + 문법 및 단어 뉘앙스의 사용을 피드백하라.
""";
    private static final String RECOMMENDATION_SYSTEM_PROMPT_TEMPLATE = """
지금부터 너의 핵심 임무는 현재 답변에 가장 어울리는 추천 표현 3개를 생성하는 것이다.
추천 표현 카테고리는 filler/adjective/adverb 각 1개이며, 사용자가 지금 말한 문맥과 의미를 직접 반영해야 한다.
문장에 어울리지 않는 상투 표현 반복은 피하고, 최근 추천 이력과 겹칠 경우 다른 표현을 우선 검토하라.
단, 문맥 적합도가 가장 높다면 중복을 허용해도 된다.

다음은 적용할 전략/룰북이다.
%s

중복 처리 지침:
%s

summary/corrections/exampleAnswer/rulebookEvidence는 간결하게 작성해도 되지만 JSON 스키마를 반드시 만족해야 한다.
""";
    private static final String RECOMMENDATION_USER_PROMPT_TEMPLATE = """
# 추천 표현 전용 생성

질문:
%s

사용자 답변:
%s

룰북 문서 1:
%s

룰북 문서 2:
%s

최근 5턴 추천 이력(중복 회피 참고):
- Filler: %s
- Adjective: %s
- Adverb: %s
""";

    private final Map<String, LlmClient> clients;
    private final RulebookUseCase rulebookUseCase;
    private final FeedbackPolicy feedbackPolicy;
    private final MetricsPort metrics;
    private final SessionStorePort sessionStore;
    private final WrongNoteUseCase wrongNoteUseCase;
    private final int conversationRebaseTurns;

    public FeedbackUseCase(List<LlmClient> clientList,
                           RulebookUseCase rulebookUseCase,
                           FeedbackPolicy feedbackPolicy,
                           MetricsPort metrics,
                           SessionStorePort sessionStore,
                           WrongNoteUseCase wrongNoteUseCase) {
        this(
                clientList,
                rulebookUseCase,
                feedbackPolicy,
                metrics,
                sessionStore,
                wrongNoteUseCase,
                DEFAULT_CONVERSATION_REBASE_TURNS
        );
    }

    public FeedbackUseCase(List<LlmClient> clientList,
                           RulebookUseCase rulebookUseCase,
                           FeedbackPolicy feedbackPolicy,
                           MetricsPort metrics,
                           SessionStorePort sessionStore,
                           WrongNoteUseCase wrongNoteUseCase,
                           int conversationRebaseTurns) {
        this.clients = clientList.stream().collect(Collectors.toMap(LlmClient::provider, c -> c));
        this.rulebookUseCase = rulebookUseCase;
        this.feedbackPolicy = feedbackPolicy;
        this.metrics = metrics;
        this.sessionStore = sessionStore;
        this.wrongNoteUseCase = wrongNoteUseCase;
        this.conversationRebaseTurns = Math.max(1, conversationRebaseTurns);
    }

    public FeedbackResult generateFeedback(String apiKey, FeedbackCommand command) {
        SessionState state = sessionStore.getOrCreate(SessionId.of(command.getSessionId()));
        FeedbackLanguage language = FeedbackLanguage.of(command.getFeedbackLanguage());
        state.setFeedbackLanguage(language);
        if (!state.shouldGenerateFeedback()) {
            return FeedbackResult.skipped();
        }
        String inputText = state.resolveFeedbackInputText(command.getText());
        PromptInput promptInput = PromptInput.fromGeneral(inputText);
        List<RulebookContext> contexts = rulebookUseCase.searchContexts(inputText);
        return FeedbackResult.generated(generateFeedbackInternal(apiKey, command, state, language, promptInput, contexts));
    }

    public Feedback generateFeedbackForTurn(String apiKey,
                                            FeedbackCommand command,
                                            String questionText,
                                            QuestionGroup questionGroup,
                                            String answerText,
                                            int maxRulebookDocuments) {
        FeedbackLanguage language = FeedbackLanguage.of(command.getFeedbackLanguage());
        SessionState state = sessionStore.getOrCreate(SessionId.of(command.getSessionId()));
        String query = buildTurnQuery(questionText, answerText);
        PromptInput promptInput = PromptInput.fromTurn(questionText, answerText);
        List<RulebookContext> contexts = rulebookUseCase.searchContextsForTurn(questionGroup, query, maxRulebookDocuments);
        return generateFeedbackInternal(apiKey, command, state, language, promptInput, contexts);
    }

    public PrefetchedTurnPrompt prefetchTurnPrompt(String questionId,
                                                   String questionText,
                                                   QuestionGroup questionGroup,
                                                   String feedbackLanguage,
                                                   int maxRulebookDocuments) {
        FeedbackLanguage language = FeedbackLanguage.of(feedbackLanguage);
        String safeQuestionText = trimText(questionText);
        String query = buildTurnQuery(safeQuestionText, "");
        List<RulebookContext> contexts = rulebookUseCase.searchContextsForTurn(questionGroup, query, maxRulebookDocuments);
        List<RulebookContext> safeContexts = safeContexts(contexts);
        String systemPrompt = buildSystemPrompt(language.value(), safeContexts);
        return new PrefetchedTurnPrompt(
                trimText(questionId),
                safeQuestionText,
                safeContexts,
                language.value(),
                systemPrompt
        );
    }

    public Feedback generateFeedbackForTurnWithPrefetch(String apiKey,
                                                        FeedbackCommand command,
                                                        String answerText,
                                                        PrefetchedTurnPrompt prefetch) {
        if (prefetch == null) {
            throw new IllegalArgumentException("prefetch is required");
        }
        FeedbackLanguage language = FeedbackLanguage.of(command.getFeedbackLanguage());
        SessionState state = sessionStore.getOrCreate(SessionId.of(command.getSessionId()));
        List<RulebookContext> safeContexts = safeContexts(prefetch.contexts());
        String safeAnswer = trimText(answerText);
        String languageValue = language.value();
        String systemPrompt = resolveSystemPrompt(prefetch, languageValue, safeContexts);
        String userPrompt = buildTurnPrompt(prefetch.questionText(), safeAnswer, safeContexts);
        return generateFeedbackFromPrompts(
                apiKey,
                command,
                state,
                language,
                safeAnswer,
                safeContexts,
                systemPrompt,
                userPrompt,
                prefetch.questionText()
        );
    }

    public void bootstrapConversation(String apiKey,
                                      FeedbackCommand command,
                                      String feedbackLanguage) throws Exception {
        if (command == null || command.getSessionId() == null || command.getSessionId().isBlank()) {
            throw new IllegalArgumentException("sessionId is required");
        }
        SessionState state = sessionStore.getOrCreate(SessionId.of(command.getSessionId()));
        if (state.isLlmBootstrapped()) {
            return;
        }
        FeedbackLanguage language = FeedbackLanguage.of(feedbackLanguage);
        String systemPrompt = buildSystemPrompt(language.value(), List.of());
        LlmClient client = Optional.ofNullable(clients.get(OPENAI_PROVIDER))
                .orElseThrow(() -> new IllegalStateException("OpenAI client is not configured"));
        LlmConversationState next = client.bootstrap(
                apiKey,
                command.getModel(),
                systemPrompt,
                state.conversationState()
        );
        state.updateConversationState(next);
        state.markLlmBootstrapped();
    }

    private Feedback generateFeedbackInternal(String apiKey,
                                              FeedbackCommand command,
                                              SessionState state,
                                              FeedbackLanguage language,
                                              PromptInput promptInput,
                                              List<RulebookContext> contexts) {
        PromptInput safeInput = promptInput == null ? PromptInput.empty() : promptInput;
        List<RulebookContext> safeContexts = safeContexts(contexts);
        String languageValue = language.value();
        String systemPrompt = buildSystemPrompt(languageValue, safeContexts);
        String userPrompt = buildUserPrompt(safeInput, safeContexts, languageValue);
        return generateFeedbackFromPrompts(
                apiKey,
                command,
                state,
                language,
                safeInput.normalizationText(),
                safeContexts,
                systemPrompt,
                userPrompt,
                safeInput.questionText()
        );
    }

    private Feedback generateFeedbackFromPrompts(String apiKey,
                                                 FeedbackCommand command,
                                                 SessionState state,
                                                 FeedbackLanguage language,
                                                 String text,
                                                 List<RulebookContext> contexts,
                                                 String systemPrompt,
                                                 String userPrompt,
                                                 String questionText) {
        String provider = normalizeProvider(command.getProvider());
        LlmClient client = Optional.ofNullable(clients.get(OPENAI_PROVIDER))
                .orElseThrow(() -> new IllegalStateException("OpenAI client is not configured"));
        SessionState safeState = state == null
                ? sessionStore.getOrCreate(SessionId.of(command.getSessionId()))
                : state;
        if (safeState.shouldRebaseConversation(conversationRebaseTurns)) {
            if (safeState.isLlmBootstrapped()) {
                // Rebase starts a fresh conversation, but keeps bootstrap already applied.
                safeState.resetConversationState(false);
            } else {
                safeState.resetConversationState();
            }
        }
        Instant start = Instant.now();
        try {
            LlmPromptContext promptContext = safeState.buildPromptContext();
            LlmGenerateResult generated = generateWithConversationRecovery(
                    client,
                    apiKey,
                    command,
                    userPrompt,
                    systemPrompt,
                    safeState,
                    promptContext
            );
            logSchemaFallbackIfNeeded(command, provider, generated.schemaFallbackReasons());
            Feedback feedback = generated.feedback();
            FeedbackConstraints constraints = feedbackConstraints();
            feedback = feedback.normalized(constraints, text, language, contexts);
            feedback = applyRecommendationStrategy(
                    apiKey,
                    command,
                    client,
                    provider,
                    language,
                    feedback,
                    constraints,
                    contexts,
                    questionText,
                    text,
                    promptContext
            );
            safeState.updateConversationState(generated.conversationState());
            safeState.setLlmSummary(resolveSummaryForNextTurn(generated, feedback));
            safeState.appendLlmTurn(questionText, text, feedback.getSummary(), RECENT_TURNS_LIMIT);
            safeState.appendLlmRecommendationTerms(
                    feedback.getRecommendations().filler().term(),
                    feedback.getRecommendations().adjective().term(),
                    feedback.getRecommendations().adverb().term(),
                    RECENT_RECOMMENDATIONS_LIMIT
            );
            metrics.recordFeedbackLatency(Duration.between(start, Instant.now()));
            wrongNoteUseCase.addFeedback(feedback);
            return feedback;
        } catch (Exception e) {
            metrics.incrementFeedbackError();
            throw new IllegalStateException("LLM feedback failed: " + failureMessage(e), e);
        }
    }

    private LlmGenerateResult generateWithConversationRecovery(LlmClient client,
                                                               String apiKey,
                                                               FeedbackCommand command,
                                                               String userPrompt,
                                                               String systemPrompt,
                                                               SessionState state,
                                                               LlmPromptContext promptContext) throws Exception {
        LlmConversationState conversationState = state.conversationState();
        boolean bootstrappedBeforeRequest = state.isLlmBootstrapped();
        String requestSystemPrompt = bootstrappedBeforeRequest ? EMPTY_SYSTEM_PROMPT : systemPrompt;
        try {
            return client.generate(
                    apiKey,
                    command.getModel(),
                    requestSystemPrompt,
                    userPrompt,
                    conversationState,
                    promptContext
            );
        } catch (Exception first) {
            if (!conversationState.hasConversationId() || !isInvalidConversationError(first)) {
                throw first;
            }
            state.resetConversationState();
            if (bootstrappedBeforeRequest) {
                LlmConversationState next = client.bootstrap(
                        apiKey,
                        command.getModel(),
                        systemPrompt,
                        state.conversationState()
                );
                state.updateConversationState(next);
                state.markLlmBootstrapped();
            }
            return client.generate(
                    apiKey,
                    command.getModel(),
                    state.isLlmBootstrapped() ? EMPTY_SYSTEM_PROMPT : systemPrompt,
                    userPrompt,
                    state.conversationState(),
                    promptContext
            );
        }
    }

    private FeedbackConstraints feedbackConstraints() {
        return new FeedbackConstraints(
                feedbackPolicy.getSummaryMaxChars(),
                feedbackPolicy.getExampleMinRatio(),
                feedbackPolicy.getExampleMaxRatio()
        );
    }

    private Feedback applyRecommendationStrategy(String apiKey,
                                                 FeedbackCommand command,
                                                 LlmClient client,
                                                 String provider,
                                                 FeedbackLanguage language,
                                                 Feedback baseFeedback,
                                                 FeedbackConstraints constraints,
                                                 List<RulebookContext> contexts,
                                                 String questionText,
                                                 String answerText,
                                                 LlmPromptContext promptContext) {
        RecommendationHistory history = recommendationHistory(promptContext);
        Recommendations selected = baseFeedback.getRecommendations();
        RecommendationEvaluation selectedEvaluation = evaluateRecommendations(selected, history);

        if (shouldGenerateRecommendationCandidate(selectedEvaluation)) {
            Recommendations candidate = generateRecommendationCandidate(
                    apiKey,
                    command,
                    client,
                    provider,
                    language,
                    constraints,
                    contexts,
                    questionText,
                    answerText,
                    history,
                    false
            );
            RecommendationEvaluation candidateEvaluation = evaluateRecommendations(candidate, history);
            if (isBetterRecommendation(candidateEvaluation, selectedEvaluation)) {
                selected = candidate;
                selectedEvaluation = candidateEvaluation;
            }

            if (shouldAttemptRecommendationRepair(selectedEvaluation)) {
                metrics.incrementRecommendationRepairAttempt();
                Recommendations repaired = generateRecommendationCandidate(
                        apiKey,
                        command,
                        client,
                        provider,
                        language,
                        constraints,
                        contexts,
                        questionText,
                        answerText,
                        history,
                        true
                );
                RecommendationEvaluation repairedEvaluation = evaluateRecommendations(repaired, history);
                if (isBetterRecommendation(repairedEvaluation, selectedEvaluation)) {
                    selected = repaired;
                    selectedEvaluation = repairedEvaluation;
                    metrics.incrementRecommendationRepairSuccess();
                }
            }
        }

        RecommendationCompletion completion = ensureRecommendationCompleteness(selected, language);
        if (completion.minimalFallbackApplied()) {
            metrics.incrementRecommendationMinimalFallback();
        }
        RecommendationEvaluation finalEvaluation = evaluateRecommendations(completion.recommendations(), history);
        if (finalEvaluation.duplicateCount() > 0) {
            metrics.incrementRecommendationDuplicateDetected();
        }
        return Feedback.of(
                baseFeedback.getSummary(),
                baseFeedback.getCorrections(),
                completion.recommendations(),
                baseFeedback.getExampleAnswer(),
                baseFeedback.getRulebookEvidence()
        );
    }

    private boolean shouldGenerateRecommendationCandidate(RecommendationEvaluation evaluation) {
        return true;
    }

    private boolean shouldAttemptRecommendationRepair(RecommendationEvaluation evaluation) {
        return evaluation.missingCount() > 0 || evaluation.duplicateCount() >= 2;
    }

    private Recommendations generateRecommendationCandidate(String apiKey,
                                                            FeedbackCommand command,
                                                            LlmClient client,
                                                            String provider,
                                                            FeedbackLanguage language,
                                                            FeedbackConstraints constraints,
                                                            List<RulebookContext> contexts,
                                                            String questionText,
                                                            String answerText,
                                                            RecommendationHistory history,
                                                            boolean strictDeduplication) {
        String systemPrompt = buildRecommendationSystemPrompt(language.value(), contexts, strictDeduplication);
        String userPrompt = buildRecommendationUserPrompt(questionText, answerText, contexts, history);
        try {
            LlmGenerateResult generated = client.generate(
                    apiKey,
                    command.getModel(),
                    systemPrompt,
                    userPrompt,
                    LlmConversationState.empty(),
                    LlmPromptContext.empty()
            );
            logSchemaFallbackIfNeeded(command, provider, generated.schemaFallbackReasons());
            Feedback normalized = generated.feedback().normalized(constraints, answerText, language, contexts);
            return normalized.getRecommendations();
        } catch (Exception error) {
            log.warning(String.format(
                    "recommendation generation failed sessionId=%s model=%s strict=%s reason=%s",
                    command.getSessionId(),
                    command.getModel(),
                    strictDeduplication,
                    failureMessage(error)
            ));
            return emptyRecommendations();
        }
    }

    private String buildRecommendationSystemPrompt(String language,
                                                   List<RulebookContext> contexts,
                                                   boolean strictDeduplication) {
        String duplicateGuideline = strictDeduplication
                ? "최근 추천어와 같은 term은 피하고, 특히 이미 중복된 항목은 다른 표현으로 교체하라."
                : "최근 추천어와의 중복을 줄이되, 현재 문맥에 가장 적합하면 중복을 허용해도 된다.";
        return RECOMMENDATION_SYSTEM_PROMPT_TEMPLATE.formatted(renderStrategy(contexts), duplicateGuideline)
                + "\n\n"
                + structuredOutputInstruction(promptLanguage(language));
    }

    private String buildRecommendationUserPrompt(String questionText,
                                                 String answerText,
                                                 List<RulebookContext> contexts,
                                                 RecommendationHistory history) {
        String safeQuestion = trimText(questionText);
        if (safeQuestion.isBlank()) {
            safeQuestion = UNKNOWN_QUESTION;
        }
        return RECOMMENDATION_USER_PROMPT_TEMPLATE.formatted(
                safeQuestion,
                trimText(answerText),
                rulebookDoc(contexts, 0),
                rulebookDoc(contexts, 1),
                joinHistory(history.fillerTerms()),
                joinHistory(history.adjectiveTerms()),
                joinHistory(history.adverbTerms())
        );
    }

    private RecommendationHistory recommendationHistory(LlmPromptContext promptContext) {
        if (promptContext == null || promptContext.recentRecommendations() == null || promptContext.recentRecommendations().isEmpty()) {
            return RecommendationHistory.empty();
        }
        Set<String> fillers = new LinkedHashSet<>();
        Set<String> adjectives = new LinkedHashSet<>();
        Set<String> adverbs = new LinkedHashSet<>();
        for (LlmPromptContext.RecommendationRecord record : promptContext.recentRecommendations()) {
            appendTerm(fillers, record.fillerTerm());
            appendTerm(adjectives, record.adjectiveTerm());
            appendTerm(adverbs, record.adverbTerm());
        }
        return new RecommendationHistory(List.copyOf(fillers), List.copyOf(adjectives), List.copyOf(adverbs));
    }

    private void appendTerm(Set<String> terms, String term) {
        String normalized = normalizeTerm(term);
        if (!normalized.isBlank()) {
            terms.add(normalized);
        }
    }

    private RecommendationEvaluation evaluateRecommendations(Recommendations recommendations,
                                                             RecommendationHistory history) {
        Recommendations safeRecommendations = recommendations == null ? emptyRecommendations() : recommendations;
        RecommendationHistory safeHistory = history == null ? RecommendationHistory.empty() : history;
        int missing = 0;
        int duplicates = 0;
        if (isRecommendationMissing(safeRecommendations.filler())) {
            missing += 1;
        } else if (isDuplicateTerm(safeRecommendations.filler().term(), safeHistory.fillerTerms())) {
            duplicates += 1;
        }
        if (isRecommendationMissing(safeRecommendations.adjective())) {
            missing += 1;
        } else if (isDuplicateTerm(safeRecommendations.adjective().term(), safeHistory.adjectiveTerms())) {
            duplicates += 1;
        }
        if (isRecommendationMissing(safeRecommendations.adverb())) {
            missing += 1;
        } else if (isDuplicateTerm(safeRecommendations.adverb().term(), safeHistory.adverbTerms())) {
            duplicates += 1;
        }
        return new RecommendationEvaluation(missing, duplicates);
    }

    private boolean isRecommendationMissing(RecommendationDetail detail) {
        if (detail == null) {
            return true;
        }
        return detail.term().isBlank() && detail.usage().isBlank();
    }

    private boolean isDuplicateTerm(String term, List<String> history) {
        if (history == null || history.isEmpty()) {
            return false;
        }
        String normalized = normalizeTerm(term);
        return !normalized.isBlank() && history.contains(normalized);
    }

    private String normalizeTerm(String term) {
        return term == null ? "" : term.trim().toLowerCase(Locale.ROOT);
    }

    private boolean isBetterRecommendation(RecommendationEvaluation candidate,
                                           RecommendationEvaluation baseline) {
        if (candidate.missingCount() != baseline.missingCount()) {
            return candidate.missingCount() < baseline.missingCount();
        }
        if (candidate.duplicateCount() != baseline.duplicateCount()) {
            return candidate.duplicateCount() < baseline.duplicateCount();
        }
        return false;
    }

    private RecommendationCompletion ensureRecommendationCompleteness(Recommendations recommendations,
                                                                      FeedbackLanguage language) {
        Recommendations safe = recommendations == null ? emptyRecommendations() : recommendations;
        boolean[] fallbackApplied = new boolean[]{false};
        RecommendationDetail filler = ensureRecommendationDetail(safe.filler(), language, "filler", fallbackApplied);
        RecommendationDetail adjective = ensureRecommendationDetail(safe.adjective(), language, "adjective", fallbackApplied);
        RecommendationDetail adverb = ensureRecommendationDetail(safe.adverb(), language, "adverb", fallbackApplied);
        return new RecommendationCompletion(
                new Recommendations(filler, adjective, adverb),
                fallbackApplied[0]
        );
    }

    private RecommendationDetail ensureRecommendationDetail(RecommendationDetail detail,
                                                            FeedbackLanguage language,
                                                            String category,
                                                            boolean[] fallbackApplied) {
        RecommendationDetail safeDetail = detail == null ? new RecommendationDetail("", "") : detail;
        if (!safeDetail.term().isBlank() || !safeDetail.usage().isBlank()) {
            return safeDetail;
        }
        fallbackApplied[0] = true;
        return new RecommendationDetail("", minimalFallbackUsage(language, category));
    }

    private String minimalFallbackUsage(FeedbackLanguage language, String category) {
        boolean english = language != null && language.isEnglish();
        if (english) {
            return switch (category) {
                case "filler" -> "No safe filler extracted in this turn. Regenerate with the next answer context.";
                case "adjective" -> "No safe adjective extracted in this turn. Regenerate with the next answer context.";
                default -> "No safe adverb extracted in this turn. Regenerate with the next answer context.";
            };
        }
        return switch (category) {
            case "filler" -> "이번 턴에서 자연스러운 필러를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다.";
            case "adjective" -> "이번 턴에서 자연스러운 형용사를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다.";
            default -> "이번 턴에서 자연스러운 부사를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다.";
        };
    }

    private Recommendations emptyRecommendations() {
        return new Recommendations(
                new RecommendationDetail("", ""),
                new RecommendationDetail("", ""),
                new RecommendationDetail("", "")
        );
    }

    private String joinHistory(List<String> terms) {
        if (terms == null || terms.isEmpty()) {
            return NO_RECENT_RECOMMENDATIONS;
        }
        return String.join(", ", terms);
    }

    private String resolveSummaryForNextTurn(LlmGenerateResult generated, Feedback feedback) {
        if (generated != null && generated.summaryForNextTurn() != null && !generated.summaryForNextTurn().isBlank()) {
            return generated.summaryForNextTurn();
        }
        return feedback == null || feedback.getSummary() == null ? "" : feedback.getSummary();
    }

    private boolean isInvalidConversationError(Exception error) {
        if (error == null) {
            return false;
        }
        String message = failureMessage(error).toLowerCase(Locale.ROOT);
        return message.contains("conversation not found")
                || message.contains("invalid conversation")
                || message.contains("unknown conversation")
                || message.contains("conversation id");
    }

    private String resolveSystemPrompt(PrefetchedTurnPrompt prefetch,
                                       String language,
                                       List<RulebookContext> contexts) {
        if (prefetch.systemPrompt() != null && language.equalsIgnoreCase(prefetch.language())) {
            return prefetch.systemPrompt();
        }
        return buildSystemPrompt(language, contexts);
    }

    private List<RulebookContext> safeContexts(List<RulebookContext> contexts) {
        if (contexts == null || contexts.isEmpty()) {
            return List.of();
        }
        return List.copyOf(contexts);
    }

    private String trimText(String text) {
        return text == null ? "" : text.trim();
    }

    private void logSchemaFallbackIfNeeded(FeedbackCommand command, String provider, List<String> reasons) {
        if (reasons == null || reasons.isEmpty()) {
            return;
        }
        metrics.incrementFeedbackSchemaFallback();
        log.warning(String.format(
                "feedback schema fallback applied sessionId=%s provider=%s model=%s reasons=%s",
                command.getSessionId(),
                provider,
                command.getModel(),
                reasons
        ));
    }

    private String normalizeProvider(String provider) {
        if (provider == null || provider.isBlank()) {
            throw new IllegalArgumentException("Provider is required");
        }
        String normalized = provider.trim().toLowerCase(Locale.ROOT);
        if (!OPENAI_PROVIDER.equals(normalized)) {
            throw new IllegalArgumentException("Unsupported provider: only openai is allowed");
        }
        return normalized;
    }

    private String buildSystemPrompt(String language, List<RulebookContext> contexts) {
        return baseCoachPrompt(renderStrategy(contexts))
                + "\n\n"
                + structuredOutputInstruction(promptLanguage(language));
    }

    private String promptLanguage(String language) {
        return "en".equalsIgnoreCase(language) ? "English" : "Korean";
    }

    private String baseCoachPrompt(String strategy) {
        return BASE_COACH_PROMPT_TEMPLATE.formatted(strategy);
    }

    private String structuredOutputInstruction(String language) {
        return STRUCTURED_OUTPUT_INSTRUCTION_TEMPLATE.formatted(language);
    }

    private String renderStrategy(List<RulebookContext> contexts) {
        if (contexts == null || contexts.isEmpty()) {
            return NO_STRATEGY;
        }
        return contexts.stream()
                .map(this::toRulebookLine)
                .collect(Collectors.joining("\n"));
    }

    private String toRulebookLine(RulebookContext context) {
        return "- [" + context.filename() + "] " + context.text();
    }

    private String buildUserPrompt(PromptInput input, List<RulebookContext> contexts, String language) {
        if (input.isTurnPrompt()) {
            return buildTurnPrompt(input, contexts);
        }
        String label = "en".equalsIgnoreCase(language) ? "User answer" : "사용자 답변";
        return label + ":\n" + input.normalizationText();
    }

    private String buildTurnPrompt(PromptInput input, List<RulebookContext> contexts) {
        return buildTurnPrompt(input.questionText(), input.answerText(), contexts);
    }

    private String buildTurnPrompt(String questionText, String answerText, List<RulebookContext> contexts) {
        String firstDoc = rulebookDoc(contexts, 0);
        String secondDoc = rulebookDoc(contexts, 1);
        return TURN_USER_PROMPT_TEMPLATE.formatted(questionText, answerText, firstDoc, secondDoc);
    }

    private String rulebookDoc(List<RulebookContext> contexts, int index) {
        if (contexts == null || index >= contexts.size()) {
            return NO_RULEBOOK_DOC;
        }
        return "[" + contexts.get(index).filename() + "] " + contexts.get(index).text();
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

    private String failureMessage(Throwable throwable) {
        Throwable current = throwable;
        String message = "";
        while (current != null) {
            if (current.getMessage() != null && !current.getMessage().isBlank()) {
                message = current.getMessage();
            }
            current = current.getCause();
        }
        return message.isBlank() ? "unknown cause" : message;
    }

    public record PrefetchedTurnPrompt(String questionId,
                                       String questionText,
                                       List<RulebookContext> contexts,
                                       String language,
                                       String systemPrompt) {
    }

    private record RecommendationHistory(List<String> fillerTerms,
                                         List<String> adjectiveTerms,
                                         List<String> adverbTerms) {
        private RecommendationHistory {
            fillerTerms = fillerTerms == null ? List.of() : List.copyOf(fillerTerms);
            adjectiveTerms = adjectiveTerms == null ? List.of() : List.copyOf(adjectiveTerms);
            adverbTerms = adverbTerms == null ? List.of() : List.copyOf(adverbTerms);
        }

        private static RecommendationHistory empty() {
            return new RecommendationHistory(List.of(), List.of(), List.of());
        }
    }

    private record RecommendationEvaluation(int missingCount, int duplicateCount) {
    }

    private record RecommendationCompletion(Recommendations recommendations, boolean minimalFallbackApplied) {
    }

    private record PromptInput(String questionText, String answerText, String generalText) {
        private static PromptInput fromGeneral(String text) {
            return new PromptInput("", "", trim(text));
        }

        private static PromptInput fromTurn(String questionText, String answerText) {
            return new PromptInput(trim(questionText), trim(answerText), "");
        }

        private static PromptInput empty() {
            return new PromptInput("", "", "");
        }

        private static String trim(String text) {
            return text == null ? "" : text.trim();
        }

        private boolean isTurnPrompt() {
            return !questionText.isBlank() || !answerText.isBlank();
        }

        private String normalizationText() {
            return isTurnPrompt() ? answerText : generalText;
        }
    }
}
