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
import java.util.logging.Logger;
import java.util.stream.Collectors;

public class FeedbackUseCase {
    private static final Logger log = Logger.getLogger(FeedbackUseCase.class.getName());
    private static final String OPENAI_PROVIDER = "openai";
    private static final int REQUIRED_CORRECTION_POINT_COUNT = 3;
    private static final int REQUIRED_RECOMMENDATION_COUNT = 3;
    private static final List<String> REQUIRED_CORRECTION_CATEGORIES = List.of(
            "Grammar", "Expression", "Logic"
    );
    private static final List<String> REQUIRED_RECOMMENDATION_CATEGORIES = List.of(
            "Filler", "Adjective", "Adverb"
    );
    private static final Map<String, List<String>> CATEGORY_KEYWORDS = Map.of(
            "Grammar", List.of("grammar", "grammer", "문법"),
            "Expression", List.of("expression", "표현"),
            "Logic", List.of("logic", "논리"),
            "Filler", List.of("filler", "필러"),
            "Adjective", List.of("adjective", "형용사"),
            "Adverb", List.of("adverb", "부사")
    );
    private static final String NO_STRATEGY = "(등록된 전략/룰북 문서가 없습니다)";
    private static final String NO_RULEBOOK_DOC = "(문서 없음)";
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
또한, 단어 사용의 자연스러움이나 문법적 오류 등을 피드백한다.

%s

위에 설명된 내용이 사용자가 취하고 있는 OPIc 고득점 전략이다.
해당 전략을 바탕으로, 사용자의 답변이 전략을 잘 지켰는지 피드백을 수행한다.
또한, 단어 사용의 자연스러움이나 문법적 오류 등을 피드백한다.
마지막으로, 사용자의 답변에 들어갈 법한 좋은 표현으로 filler words와 adjective, adverb를 각각 추천하고 사용법을 설명하라.

---
이제 본격적으로 학습을 도와라.
""";
    private static final String STRICT_JSON_INSTRUCTION_TEMPLATE = """
반드시 JSON 객체 1개만 출력하라. 마크다운/코드펜스/설명문/주석을 출력하지 마라.
출력 키는 정확히 summary, correctionPoints, recommendation, exampleAnswer, rulebookEvidence 5개만 허용한다.
JSON 타입 계약(반드시 동일하게 준수):
{
  "summary": "string",
  "correctionPoints": ["string", "string", "string"],
  "recommendation": ["string", "string", "string"],
  "exampleAnswer": "string",
  "rulebookEvidence": ["string", "..."]
}
- summary: 1~2문장
- correctionPoints: 정확히 3개, Grammar/Expression/Logic를 각각 1개씩 포함
- correctionPoints 순서: Grammar, Expression, Logic
- recommendation: 정확히 3개, Filler/Adjective/Adverb를 각각 1개씩 포함
- recommendation 순서: Filler, Adjective, Adverb
- correctionPoints는 반드시 JSON 배열([])이어야 한다. 객체({})/문자열/숫자/null 금지
- recommendation은 반드시 JSON 배열([])이어야 한다. 객체({})/문자열/숫자/null 금지
- 각 correctionPoints 항목은 반드시 "Category: 설명" 형식으로 작성
- 각 recommendation 항목은 반드시 "Category: 설명" 형식으로 작성
- exampleAnswer: 사용자 답변 길이의 0.8~1.2배
- rulebookEvidence: 룰북 근거 배열(근거가 없으면 빈 배열)
아래 규칙을 모두 만족하지 못하면 출력하지 말고 내부에서 다시 생성하라:
1) correctionPoints[0]은 반드시 "Grammar:"로 시작
2) correctionPoints[1]은 반드시 "Expression:"로 시작
3) correctionPoints[2]은 반드시 "Logic:"로 시작
4) recommendation[0]은 반드시 "Filler:"로 시작
5) recommendation[1]는 반드시 "Adjective:"로 시작
6) recommendation[2]는 반드시 "Adverb:"로 시작
7) correctionPoints/recommendation 외에 grammar/grammer/expression/logic/filler/adjective/adverb 같은 개별 키를 절대 만들지 말 것
8) summary/correctionPoints/recommendation/exampleAnswer/rulebookEvidence 외의 추가 키를 절대 만들지 말 것
9) correctionPoints/recommendation의 각 원소 타입은 반드시 문자열이어야 함
출력 직전 자기검증 체크리스트:
- JSON parse 가능
- 키 5개만 존재
- correctionPoints 길이 정확히 3
- recommendation 길이 정확히 3
- correctionPoints는 배열 타입
- recommendation은 배열 타입
- 접두사/순서 정확히 일치
모든 본문 텍스트는 %s로 작성하라.
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
        PromptInput promptInput = PromptInput.fromGeneral(inputText);
        List<RulebookContext> contexts = rulebookUseCase.searchContexts(inputText);
        return FeedbackResult.generated(generateFeedbackInternal(apiKey, command, language, promptInput, contexts));
    }

    public Feedback generateFeedbackForTurn(String apiKey,
                                            FeedbackCommand command,
                                            String questionText,
                                            QuestionGroup questionGroup,
                                            String answerText,
                                            int maxRulebookDocuments) {
        FeedbackLanguage language = FeedbackLanguage.of(command.getFeedbackLanguage());
        String query = buildTurnQuery(questionText, answerText);
        PromptInput promptInput = PromptInput.fromTurn(questionText, answerText);
        List<RulebookContext> contexts = rulebookUseCase.searchContextsForTurn(questionGroup, query, maxRulebookDocuments);
        return generateFeedbackInternal(apiKey, command, language, promptInput, contexts);
    }

    private Feedback generateFeedbackInternal(String apiKey,
                                              FeedbackCommand command,
                                              FeedbackLanguage language,
                                              PromptInput promptInput,
                                              List<RulebookContext> contexts) {
        PromptInput safeInput = promptInput == null ? PromptInput.empty() : promptInput;
        String text = safeInput.normalizationText();
        List<RulebookContext> safeContexts = contexts == null ? List.of() : contexts;
        Instant start = Instant.now();
        String systemPrompt = buildSystemPrompt(language.value(), safeContexts);
        String userPrompt = buildUserPrompt(safeInput, safeContexts, language.value());

        String provider = normalizeProvider(command.getProvider());
        LlmClient client = Optional.ofNullable(clients.get(OPENAI_PROVIDER))
                .orElseThrow(() -> new IllegalStateException("OpenAI client is not configured"));

        try {
            String raw = client.generate(apiKey, command.getModel(), systemPrompt, userPrompt);
            ParsedFeedback parsed = parseFeedback(raw);
            logSchemaFallbackIfNeeded(command, provider, parsed.schemaFallbackReasons());
            Feedback feedback = parsed.feedback();
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
            throw new IllegalStateException("LLM feedback failed: " + failureMessage(e), e);
        }
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
                + strictJsonInstruction(promptLanguage(language));
    }

    private String promptLanguage(String language) {
        return "en".equalsIgnoreCase(language) ? "English" : "Korean";
    }

    private String baseCoachPrompt(String strategy) {
        return BASE_COACH_PROMPT_TEMPLATE.formatted(strategy);
    }

    private String strictJsonInstruction(String language) {
        return STRICT_JSON_INSTRUCTION_TEMPLATE.formatted(language);
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
        String firstDoc = rulebookDoc(contexts, 0);
        String secondDoc = rulebookDoc(contexts, 1);
        return TURN_USER_PROMPT_TEMPLATE.formatted(input.questionText(), input.answerText(), firstDoc, secondDoc);
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

    private ParsedFeedback parseFeedback(String raw) throws Exception {
        JsonNode root = objectMapper.readTree(extractJson(raw));
        String summary = root.path("summary").asText("");
        JsonNode correctionNode = root.path("correctionPoints");
        List<String> points = toStringList(correctionNode);
        JsonNode recommendationNode = root.path("recommendation");
        List<String> recommendations = toStringList(recommendationNode);
        String exampleAnswer = root.path("exampleAnswer").asText("");
        List<String> evidence = toStringList(root.path("rulebookEvidence"));
        List<String> reasons = schemaFallbackReasons(correctionNode, points, recommendationNode, recommendations);
        return new ParsedFeedback(Feedback.of(summary, points, recommendations, exampleAnswer, evidence), reasons);
    }

    private List<String> schemaFallbackReasons(JsonNode correctionNode,
                                               List<String> points,
                                               JsonNode recommendationNode,
                                               List<String> recommendations) {
        List<String> reasons = new ArrayList<>();
        collectArrayValidationReasons(
                correctionNode,
                points,
                "correctionPoints",
                REQUIRED_CORRECTION_POINT_COUNT,
                REQUIRED_CORRECTION_CATEGORIES,
                reasons
        );
        collectArrayValidationReasons(
                recommendationNode,
                recommendations,
                "recommendation",
                REQUIRED_RECOMMENDATION_COUNT,
                REQUIRED_RECOMMENDATION_CATEGORIES,
                reasons
        );
        return reasons;
    }

    private void collectArrayValidationReasons(JsonNode node,
                                               List<String> values,
                                               String fieldName,
                                               int requiredCount,
                                               List<String> requiredCategories,
                                               List<String> reasons) {
        if (node == null || node.isMissingNode() || !node.isArray()) {
            reasons.add(fieldName + "_not_array");
            return;
        }
        if (values.isEmpty()) {
            reasons.add(fieldName + "_empty");
        }
        if (values.size() != requiredCount) {
            reasons.add(fieldName + "_count_" + values.size());
        }
        for (String category : requiredCategories) {
            if (!hasCategory(values, category)) {
                reasons.add("missing_" + fieldName + "_" + category.toLowerCase(Locale.ROOT));
            }
        }
    }

    private boolean hasCategory(List<String> points, String category) {
        return points.stream().anyMatch(point -> hasCategory(point, category));
    }

    private boolean hasCategory(String point, String category) {
        if (point == null || category == null) {
            return false;
        }
        String lowered = point.toLowerCase(Locale.ROOT);
        List<String> keywords = CATEGORY_KEYWORDS.getOrDefault(category, List.of());
        return keywords.stream().anyMatch(lowered::contains);
    }

    private List<String> toStringList(JsonNode arrayNode) {
        List<String> values = new ArrayList<>();
        if (arrayNode == null || !arrayNode.isArray()) {
            return values;
        }
        for (JsonNode node : arrayNode) {
            values.add(node.asText());
        }
        return values;
    }

    private String extractJson(String raw) {
        int start = raw.indexOf('{');
        int end = raw.lastIndexOf('}');
        if (start >= 0 && end > start) {
            return raw.substring(start, end + 1);
        }
        return raw;
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

    private record ParsedFeedback(Feedback feedback, List<String> schemaFallbackReasons) {
    }
}
