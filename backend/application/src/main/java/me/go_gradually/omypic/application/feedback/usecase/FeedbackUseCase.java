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

---
이제 본격적으로 학습을 도와라.
""";
    private static final String STRICT_JSON_INSTRUCTION_TEMPLATE = """
반드시 JSON만 출력하라. 마크다운/코드펜스/설명문을 출력하지 마라.
출력 키는 정확히 summary, correctionPoints, exampleAnswer, rulebookEvidence 4개만 사용하라.
- summary: 1~2문장
- correctionPoints: 정확히 3개, Grammar/Expression/Logic 중 최소 2종 포함
- exampleAnswer: 사용자 답변 길이의 0.8~1.2배
- rulebookEvidence: 룰북 근거 배열(근거가 없으면 빈 배열)
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
            throw new IllegalStateException("LLM feedback failed: " + failureMessage(e), e);
        }
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

    private Feedback parseFeedback(String raw) throws Exception {
        JsonNode root = objectMapper.readTree(extractJson(raw));
        String summary = root.path("summary").asText("");
        List<String> points = toStringList(root.path("correctionPoints"));
        String exampleAnswer = root.path("exampleAnswer").asText("");
        List<String> evidence = toStringList(root.path("rulebookEvidence"));
        return Feedback.of(summary, points, exampleAnswer, evidence);
    }

    private List<String> toStringList(JsonNode arrayNode) {
        List<String> values = new ArrayList<>();
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
}
