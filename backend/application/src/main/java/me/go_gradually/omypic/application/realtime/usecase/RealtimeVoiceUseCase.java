package me.go_gradually.omypic.application.realtime.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.application.realtime.model.*;
import me.go_gradually.omypic.application.realtime.policy.RealtimePolicy;
import me.go_gradually.omypic.application.realtime.port.RealtimeAudioGateway;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionFlowPolicy;
import me.go_gradually.omypic.domain.session.SessionStopReason;
import me.go_gradually.omypic.domain.session.SessionState;
import me.go_gradually.omypic.domain.session.TurnBatchingPolicy;

import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Deque;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedDeque;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

public class RealtimeVoiceUseCase {
    private final RealtimeAudioGateway realtimeAudioGateway;
    private final FeedbackUseCase feedbackUseCase;
    private final SessionUseCase sessionUseCase;
    private final QuestionUseCase questionUseCase;
    private final AsyncExecutor asyncExecutor;
    private final RealtimePolicy realtimePolicy;
    private final MetricsPort metrics;

    public RealtimeVoiceUseCase(RealtimeAudioGateway realtimeAudioGateway,
                                FeedbackUseCase feedbackUseCase,
                                SessionUseCase sessionUseCase,
                                QuestionUseCase questionUseCase,
                                AsyncExecutor asyncExecutor,
                                RealtimePolicy realtimePolicy,
                                MetricsPort metrics) {
        this.realtimeAudioGateway = realtimeAudioGateway;
        this.feedbackUseCase = feedbackUseCase;
        this.sessionUseCase = sessionUseCase;
        this.questionUseCase = questionUseCase;
        this.asyncExecutor = asyncExecutor;
        this.realtimePolicy = realtimePolicy;
        this.metrics = metrics;
    }

    private static boolean isBlank(String value) {
        return value == null || value.isBlank();
    }

    private static String firstNonBlank(String candidate, String fallback) {
        return isBlank(candidate) ? fallback : candidate;
    }

    public RealtimeVoiceSession open(RealtimeStartCommand command, RealtimeVoiceEventSink sink) {
        if (command == null || isBlank(command.getSessionId()) || isBlank(command.getApiKey())) {
            throw new IllegalArgumentException("sessionId and apiKey are required");
        }

        sessionUseCase.getOrCreate(command.getSessionId());
        RuntimeSettings defaults = RuntimeSettings.defaults(realtimePolicy);
        RuntimeSettings settings = defaults.withRealtimeModels(
                firstNonBlank(command.getConversationModel(), defaults.conversationModel()),
                firstNonBlank(command.getSttModel(), defaults.sttModel())
        );
        RuntimeContext context = new RuntimeContext(command.getSessionId(), command.getApiKey(), sink, settings);

        RealtimeAudioSession realtimeAudioSession = realtimeAudioGateway.open(
                new RealtimeAudioOpenCommand(command.getApiKey(), settings.conversationModel(), settings.sttModel()),
                new RealtimeAudioEventListener() {
                    @Override
                    public void onPartialTranscript(String delta) {
                        if (context.closed.get() || context.forcedStopped.get() || isBlank(delta)) {
                            return;
                        }
                        context.send("stt.partial", Map.of("sessionId", context.sessionId, "text", delta));
                    }

                    @Override
                    public void onFinalTranscript(String text) {
                        if (context.closed.get() || context.forcedStopped.get() || isBlank(text)) {
                            return;
                        }
                        handleFinalTranscript(context, text);
                    }

                    @Override
                    public void onAssistantAudioChunk(long turnId, String base64Audio) {
                        if (context.closed.get() || context.forcedStopped.get() || isBlank(base64Audio) || isCancelled(context, turnId)) {
                            return;
                        }
                        context.send("tts.chunk", Map.of("sessionId", context.sessionId, "turnId", turnId, "audio", base64Audio));
                    }

                    @Override
                    public void onAssistantAudioCompleted(long turnId) {
                        context.completeSpeech(turnId);
                    }

                    @Override
                    public void onAssistantAudioFailed(long turnId, String message) {
                        context.completeSpeech(turnId);
                        metrics.incrementTtsError();
                        if (context.closed.get()) {
                            return;
                        }
                        context.send("tts.error", Map.of(
                                "sessionId", context.sessionId,
                                "turnId", turnId,
                                "message", defaultMessage(message)
                        ));
                    }

                    @Override
                    public void onError(String message) {
                        if (context.closed.get()) {
                            return;
                        }
                        context.send("error", Map.of("sessionId", context.sessionId, "message", defaultMessage(message)));
                    }
                }
        );
        context.audioSession = realtimeAudioSession;
        context.send("connection.ready", Map.of("sessionId", context.sessionId));

        asyncExecutor.execute(() -> sendInitialQuestion(context));
        return context;
    }

    private void sendInitialQuestion(RuntimeContext context) {
        if (context.closed.get() || context.forcedStopped.get()) {
            return;
        }
        long turnId = context.nextTurnId();
        try {
            SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
            NextQuestion nextQuestion = requestNextQuestion(context, turnId, sessionState);
            if (isQuestionExhausted(nextQuestion)) {
                context.requestAutoStop(turnId, SessionStopReason.QUESTION_EXHAUSTED.code());
            } else if (!isBlank(nextQuestion.getText())) {
                streamSpeech(context, turnId, nextQuestion.getText());
            }
        } catch (Exception e) {
            context.send("error", Map.of("sessionId", context.sessionId, "message", defaultMessage(e.getMessage())));
        } finally {
            context.markTurnProcessingDone(turnId);
        }
    }

    private void handleFinalTranscript(RuntimeContext context, String text) {
        long turnId = context.nextTurnId();
        TurnInput turnInput = context.captureTurnInput(text);
        context.activeTurn.set(turnId);
        context.send("stt.final", Map.of("sessionId", context.sessionId, "turnId", turnId, "text", text));
        sessionUseCase.appendSegment(context.sessionId, text);
        Instant startedAt = Instant.now();
        asyncExecutor.execute(() -> processTurn(context, turnId, turnInput, startedAt));
    }

    private void processTurn(RuntimeContext context, long turnId, TurnInput turnInput, Instant startedAt) {
        RuntimeSettings settings = context.settings;
        try {
            if (context.closed.get() || context.forcedStopped.get() || isCancelled(context, turnId)) {
                return;
            }

            SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
            ModeType mode = sessionState.getMode();
            sessionState.setFeedbackLanguage(FeedbackLanguage.of(settings.feedbackLanguage));
            String feedbackApiKey = firstNonBlank(settings.feedbackApiKey, context.apiKey);

            List<TurnInput> feedbackInputs = List.of();
            TurnBatchingPolicy.BatchReason batchReason = TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE;
            boolean residualBatch = false;
            if (mode == ModeType.IMMEDIATE) {
                feedbackInputs = List.of(turnInput);
                batchReason = TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE;
            } else if (mode == ModeType.CONTINUOUS) {
                context.appendContinuousTurn(turnInput);
                TurnBatchingPolicy.BatchDecision decision = sessionState.decideFeedbackBatchOnAnswer();
                batchReason = decision.reason();
                if (decision.emitFeedback()) {
                    feedbackInputs = context.pollContinuousTurns(sessionState.getContinuousBatchSize());
                } else {
                    context.send("feedback.skipped", Map.of(
                            "sessionId", context.sessionId,
                            "turnId", turnId,
                            "reason", decision.reason().name()
                    ));
                }
            } else if (mode == ModeType.MOCK_EXAM) {
                context.appendMockTurn(turnInput);
                batchReason = TurnBatchingPolicy.BatchReason.WAITING_FOR_MOCK_EXAM_COMPLETION;
            }

            NextQuestion nextQuestion = null;
            boolean questionExhausted = false;
            SessionFlowPolicy.SessionAction nextAction = SessionFlowPolicy.SessionAction.askNext();
            if (shouldPromptNextQuestion(mode) && !context.closed.get() && !context.forcedStopped.get()) {
                nextQuestion = requestNextQuestion(context, turnId, sessionState);
                questionExhausted = isQuestionExhausted(nextQuestion);
                nextAction = SessionFlowPolicy.decideAfterQuestionSelection(questionExhausted);
            }

            if (!feedbackInputs.isEmpty()) {
                List<FeedbackItem> feedbackItems = generateFeedbackItems(settings, context.sessionId, feedbackApiKey, feedbackInputs);
                if (!context.closed.get() && !context.forcedStopped.get() && !isCancelled(context, turnId)) {
                    context.send("feedback.final", feedbackPayload(
                            context.sessionId,
                            turnId,
                            mode,
                            mode == ModeType.CONTINUOUS ? sessionState.getContinuousBatchSize() : feedbackItems.size(),
                            batchReason,
                            residualBatch,
                            feedbackItems,
                            nextAction
                    ));
                    streamSpeech(context, turnId, toTtsText(feedbackItems));
                }
            }

            if (mode == ModeType.MOCK_EXAM && nextQuestion != null && nextQuestion.isMockExamCompleted() && !context.forcedStopped.get()) {
                if (!sessionState.isMockFinalFeedbackGenerated()) {
                    List<TurnInput> mockTurns = context.pollMockTurns();
                    if (!mockTurns.isEmpty()) {
                        List<FeedbackItem> mockFinalItems = generateFeedbackItems(settings, context.sessionId, feedbackApiKey, mockTurns);
                        if (!isCancelled(context, turnId) && !context.closed.get() && !context.forcedStopped.get()) {
                            context.send("feedback.final", feedbackPayload(
                                    context.sessionId,
                                    turnId,
                                    mode,
                                    mockFinalItems.size(),
                                    TurnBatchingPolicy.BatchReason.MOCK_EXAM_FINAL,
                                    false,
                                    mockFinalItems,
                                    nextAction
                            ));
                            streamSpeech(context, turnId, toTtsText(mockFinalItems));
                        }
                    }
                    sessionState.markMockFinalFeedbackGenerated();
                }
            }

            if (sessionState.shouldGenerateResidualContinuousFeedback(questionExhausted, !feedbackInputs.isEmpty())) {
                List<TurnInput> remainingContinuousTurns = context.pollAllContinuousTurns();
                if (!remainingContinuousTurns.isEmpty()) {
                    residualBatch = true;
                    List<FeedbackItem> remainingItems = generateFeedbackItems(settings, context.sessionId, feedbackApiKey, remainingContinuousTurns);
                    if (!context.closed.get() && !context.forcedStopped.get() && !isCancelled(context, turnId)) {
                        context.send("feedback.final", feedbackPayload(
                                context.sessionId,
                                turnId,
                                mode,
                                sessionState.getContinuousBatchSize(),
                                TurnBatchingPolicy.BatchReason.EXHAUSTED_WITH_REMAINDER,
                                residualBatch,
                                remainingItems,
                                nextAction
                        ));
                        streamSpeech(context, turnId, toTtsText(remainingItems));
                    }
                }
            }

            if (nextQuestion != null && !questionExhausted && !isBlank(nextQuestion.getText()) && !isCancelled(context, turnId)) {
                streamSpeech(context, turnId, nextQuestion.getText());
            }

            if (nextAction.type() == SessionFlowPolicy.NextActionType.AUTO_STOP) {
                context.requestAutoStop(turnId, nextAction.reason().code());
            }

        } catch (Exception e) {
            context.send("error", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "message", defaultMessage(e.getMessage())
            ));
        } finally {
            context.markTurnProcessingDone(turnId);
            metrics.recordRealtimeTurnLatency(Duration.between(startedAt, Instant.now()));
        }
    }

    private boolean shouldPromptNextQuestion(ModeType mode) {
        if (mode == ModeType.IMMEDIATE) {
            return true;
        }
        if (mode == ModeType.CONTINUOUS) {
            return true;
        }
        return mode == ModeType.MOCK_EXAM;
    }

    private boolean isQuestionExhausted(NextQuestion nextQuestion) {
        return nextQuestion == null || nextQuestion.isSkipped();
    }

    private NextQuestion requestNextQuestion(RuntimeContext context, long turnId, SessionState sessionState) {
        String listId = resolveActiveQuestionListId(sessionState);
        NextQuestion nextQuestion = questionUseCase.nextQuestion(listId, context.sessionId);
        context.updateCurrentQuestion(nextQuestion);
        context.send("question.prompt", questionPayload(context.sessionId, turnId, sessionState.getMode(), nextQuestion));
        return nextQuestion;
    }

    private String resolveActiveQuestionListId(SessionState state) {
        String listId = state.getActiveQuestionListId();
        if (isBlank(listId)) {
            throw new IllegalStateException("질문 리스트를 먼저 선택하고 학습 모드를 적용해 주세요.");
        }
        return listId;
    }

    private FeedbackCommand feedbackCommand(RuntimeSettings settings, String sessionId, String text) {
        FeedbackCommand feedbackCommand = new FeedbackCommand();
        feedbackCommand.setSessionId(sessionId);
        feedbackCommand.setText(text);
        feedbackCommand.setProvider(settings.feedbackProvider);
        feedbackCommand.setModel(settings.feedbackModel);
        feedbackCommand.setFeedbackLanguage(settings.feedbackLanguage);
        return feedbackCommand;
    }

    private List<FeedbackItem> generateFeedbackItems(RuntimeSettings settings,
                                                     String sessionId,
                                                     String apiKey,
                                                     List<TurnInput> turns) {
        List<FeedbackItem> items = new ArrayList<>();
        for (TurnInput turn : turns) {
            Feedback feedback = feedbackUseCase.generateFeedbackForTurn(
                    apiKey,
                    feedbackCommand(settings, sessionId, turn.answerText()),
                    turn.questionText(),
                    turn.questionGroup(),
                    turn.answerText(),
                    2
            );
            items.add(new FeedbackItem(
                    turn.questionId(),
                    turn.questionText(),
                    turn.questionGroup(),
                    turn.answerText(),
                    feedback
            ));
        }
        return items;
    }

    private Map<String, Object> questionPayload(String sessionId, long turnId, ModeType mode, NextQuestion question) {
        boolean exhausted = isQuestionExhausted(question);
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("sessionId", sessionId);
        payload.put("turnId", turnId);
        payload.put("question", questionNode(question, exhausted));

        Map<String, Object> selection = new LinkedHashMap<>();
        selection.put("mode", mode == null ? ModeType.IMMEDIATE.name() : mode.name());
        selection.put("exhausted", exhausted);
        if (exhausted) {
            selection.put("reason", resolveExhaustedReason(question));
        }
        payload.put("selection", selection);
        return payload;
    }

    private Map<String, Object> questionNode(NextQuestion question, boolean exhausted) {
        if (question == null || exhausted) {
            return null;
        }
        Map<String, Object> node = new LinkedHashMap<>();
        node.put("id", question.getQuestionId());
        node.put("text", question.getText());
        node.put("group", question.getGroup());
        return node;
    }

    private Map<String, Object> feedbackPayload(String sessionId,
                                                long turnId,
                                                ModeType mode,
                                                int batchSize,
                                                TurnBatchingPolicy.BatchReason reason,
                                                boolean residual,
                                                List<FeedbackItem> items,
                                                SessionFlowPolicy.SessionAction nextAction) {
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("sessionId", sessionId);
        payload.put("turnId", turnId);
        payload.put("policy", feedbackPolicyPayload(mode, batchSize, reason));
        payload.put("batch", feedbackBatchPayload(items, residual));
        payload.put("nextAction", nextActionPayload(nextAction));
        return payload;
    }

    private Map<String, Object> feedbackPolicyPayload(ModeType mode, int batchSize, TurnBatchingPolicy.BatchReason reason) {
        Map<String, Object> policy = new LinkedHashMap<>();
        policy.put("mode", mode == null ? ModeType.IMMEDIATE.name() : mode.name());
        policy.put("batchSize", Math.max(1, batchSize));
        policy.put("reason", reason == null ? "" : reason.name());
        return policy;
    }

    private Map<String, Object> feedbackBatchPayload(List<FeedbackItem> items, boolean residual) {
        Map<String, Object> batch = new LinkedHashMap<>();
        List<FeedbackItem> safeItems = items == null ? List.of() : items;
        batch.put("size", safeItems.size());
        batch.put("isResidual", residual);
        batch.put("items", safeItems.stream().map(this::feedbackItemPayload).toList());
        return batch;
    }

    private Map<String, Object> nextActionPayload(SessionFlowPolicy.SessionAction action) {
        SessionFlowPolicy.SessionAction safeAction = action == null ? SessionFlowPolicy.SessionAction.askNext() : action;
        Map<String, Object> nextAction = new LinkedHashMap<>();
        nextAction.put("type", safeAction.type() == SessionFlowPolicy.NextActionType.AUTO_STOP ? "auto_stop" : "ask_next");
        if (safeAction.reason() != null) {
            nextAction.put("reason", safeAction.reason().code());
        }
        return nextAction;
    }

    private String resolveExhaustedReason(NextQuestion question) {
        if (question != null && !isBlank(question.getMockExamCompletionReason())) {
            return question.getMockExamCompletionReason();
        }
        return SessionStopReason.QUESTION_EXHAUSTED.code();
    }

    private void streamSpeech(RuntimeContext context, long turnId, String speechText) {
        if (isBlank(speechText) || context.closed.get() || context.forcedStopped.get() || isCancelled(context, turnId)) {
            return;
        }
        RealtimeAudioSession audioSession = context.audioSession;
        if (audioSession == null) {
            metrics.incrementTtsError();
            context.send("tts.error", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "message", "Realtime audio session is not available"
            ));
            return;
        }
        context.registerSpeech(turnId);
        try {
            RuntimeSettings settings = context.settings;
            audioSession.speakText(turnId, speechText, settings.ttsVoice);
        } catch (Exception e) {
            context.completeSpeech(turnId);
            metrics.incrementTtsError();
            context.send("tts.error", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "message", defaultMessage(e.getMessage())
            ));
        }
    }

    private Map<String, Object> feedbackItemPayload(FeedbackItem item) {
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("questionId", item.questionId());
        payload.put("questionText", item.questionText());
        payload.put("questionGroup", item.questionGroup() == null ? null : item.questionGroup().value());
        payload.put("answerText", item.answerText());
        payload.put("summary", item.feedback().getSummary());
        payload.put("correctionPoints", item.feedback().getCorrectionPoints());
        payload.put("exampleAnswer", item.feedback().getExampleAnswer());
        payload.put("rulebookEvidence", item.feedback().getRulebookEvidence());
        return payload;
    }

    private String toTtsText(Feedback feedback) {
        StringBuilder sb = new StringBuilder();
        if (!isBlank(feedback.getSummary())) {
            sb.append(feedback.getSummary()).append('\n');
        }
        if (!feedback.getCorrectionPoints().isEmpty()) {
            sb.append(String.join("\n", feedback.getCorrectionPoints())).append('\n');
        }
        if (!isBlank(feedback.getExampleAnswer())) {
            sb.append(feedback.getExampleAnswer());
        }
        String text = sb.toString().trim();
        if (text.isEmpty()) {
            return "No feedback available.";
        }
        return text;
    }

    private String toTtsText(List<FeedbackItem> items) {
        if (items == null || items.isEmpty()) {
            return "No feedback available.";
        }
        if (items.size() == 1) {
            return toTtsText(items.get(0).feedback());
        }
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < items.size(); i++) {
            FeedbackItem item = items.get(i);
            sb.append("Feedback ").append(i + 1).append('\n');
            if (!isBlank(item.questionText())) {
                sb.append(item.questionText()).append('\n');
            }
            sb.append(toTtsText(item.feedback())).append('\n');
        }
        return sb.toString().trim();
    }

    private boolean isCancelled(RuntimeContext context, long turnId) {
        return context.cancelledThroughTurn.get() >= turnId;
    }

    private String defaultMessage(String message) {
        return isBlank(message) ? "Unknown realtime error" : message;
    }

    private record TurnInput(String questionId, String questionText, QuestionGroup questionGroup, String answerText) {
        private static TurnInput fromQuestion(NextQuestion question) {
            if (question == null) {
                return empty();
            }
            return new TurnInput(
                    safe(question.getQuestionId()),
                    safe(question.getText()),
                    QuestionGroup.fromNullable(question.getGroup()),
                    ""
            );
        }

        private static TurnInput empty() {
            return new TurnInput("", "", null, "");
        }

        private static String safe(String value) {
            return value == null ? "" : value;
        }
    }

    private record FeedbackItem(String questionId,
                                String questionText,
                                QuestionGroup questionGroup,
                                String answerText,
                                Feedback feedback) {
    }

    private record RuntimeSettings(String conversationModel, String sttModel,
                                   String feedbackProvider, String feedbackModel, String feedbackApiKey,
                                   String feedbackLanguage, String ttsVoice) {

        private static RuntimeSettings defaults(RealtimePolicy policy) {
            return new RuntimeSettings(
                    policy.realtimeConversationModel(),
                    policy.realtimeSttModel(),
                    policy.realtimeFeedbackProvider(),
                    policy.realtimeFeedbackModel(),
                    null,
                    policy.realtimeFeedbackLanguage(),
                    policy.realtimeTtsVoice()
            );
        }

        private RuntimeSettings apply(RealtimeSessionUpdateCommand command) {
            return new RuntimeSettings(
                    firstNonBlank(command.getConversationModel(), conversationModel),
                    firstNonBlank(command.getSttModel(), sttModel),
                    firstNonBlank(command.getFeedbackProvider(), feedbackProvider),
                    firstNonBlank(command.getFeedbackModel(), feedbackModel),
                    firstNonBlank(command.getFeedbackApiKey(), feedbackApiKey),
                    firstNonBlank(command.getFeedbackLanguage(), feedbackLanguage),
                    firstNonBlank(command.getTtsVoice(), ttsVoice)
            );
        }

        private RuntimeSettings withRealtimeModels(String conversationModel, String sttModel) {
            return new RuntimeSettings(
                    conversationModel,
                    sttModel,
                    feedbackProvider,
                    feedbackModel,
                    feedbackApiKey,
                    feedbackLanguage,
                    ttsVoice
            );
        }
    }

    private static final class RuntimeContext implements RealtimeVoiceSession {
        private final String sessionId;
        private final String apiKey;
        private final RealtimeVoiceEventSink sink;
        private final AtomicLong turnSequence = new AtomicLong(0);
        private final AtomicLong activeTurn = new AtomicLong(0);
        private final AtomicLong cancelledThroughTurn = new AtomicLong(0);
        private final AtomicLong autoStopAfterTurn = new AtomicLong(-1L);
        private final Deque<TurnInput> continuousTurns = new ConcurrentLinkedDeque<>();
        private final Deque<TurnInput> mockTurns = new ConcurrentLinkedDeque<>();
        private final Map<Long, AtomicLong> pendingSpeechCounts = new ConcurrentHashMap<>();
        private final Set<Long> turnsReadyForCompletion = ConcurrentHashMap.newKeySet();
        private final Set<Long> completedTurns = ConcurrentHashMap.newKeySet();
        private final AtomicBoolean closed = new AtomicBoolean(false);
        private final AtomicBoolean forcedStopped = new AtomicBoolean(false);
        private final AtomicBoolean autoStopPending = new AtomicBoolean(false);
        private volatile TurnInput currentQuestion = TurnInput.empty();
        private volatile String autoStopReason = "";
        private volatile RuntimeSettings settings;
        private volatile RealtimeAudioSession audioSession;

        private RuntimeContext(String sessionId, String apiKey, RealtimeVoiceEventSink sink, RuntimeSettings settings) {
            this.sessionId = sessionId;
            this.apiKey = apiKey;
            this.sink = sink;
            this.settings = settings;
        }

        private long nextTurnId() {
            return turnSequence.incrementAndGet();
        }

        private void updateCurrentQuestion(NextQuestion question) {
            currentQuestion = TurnInput.fromQuestion(question);
        }

        private TurnInput captureTurnInput(String answerText) {
            TurnInput question = currentQuestion;
            return new TurnInput(
                    question.questionId(),
                    question.questionText(),
                    question.questionGroup(),
                    answerText == null ? "" : answerText.trim()
            );
        }

        private void appendContinuousTurn(TurnInput turnInput) {
            continuousTurns.addLast(turnInput);
        }

        private List<TurnInput> pollContinuousTurns(int batchSize) {
            int size = Math.max(1, batchSize);
            List<TurnInput> batch = new ArrayList<>();
            while (batch.size() < size) {
                TurnInput item = continuousTurns.pollFirst();
                if (item == null) {
                    break;
                }
                batch.add(item);
            }
            return batch;
        }

        private List<TurnInput> pollAllContinuousTurns() {
            List<TurnInput> turns = new ArrayList<>();
            TurnInput item;
            while ((item = continuousTurns.pollFirst()) != null) {
                turns.add(item);
            }
            return turns;
        }

        private void appendMockTurn(TurnInput turnInput) {
            mockTurns.addLast(turnInput);
        }

        private List<TurnInput> pollMockTurns() {
            List<TurnInput> turns = new ArrayList<>();
            TurnInput item;
            while ((item = mockTurns.pollFirst()) != null) {
                turns.add(item);
            }
            return turns;
        }

        private void registerSpeech(long turnId) {
            pendingSpeechCounts.computeIfAbsent(turnId, ignored -> new AtomicLong()).incrementAndGet();
        }

        private void completeSpeech(long turnId) {
            AtomicLong pending = pendingSpeechCounts.get(turnId);
            if (pending == null) {
                tryCompleteTurn(turnId);
                return;
            }
            long remaining = pending.decrementAndGet();
            if (remaining <= 0) {
                pendingSpeechCounts.remove(turnId);
            }
            tryCompleteTurn(turnId);
        }

        private void markTurnProcessingDone(long turnId) {
            turnsReadyForCompletion.add(turnId);
            tryCompleteTurn(turnId);
        }

        private void requestAutoStop(long turnId, String reason) {
            if (turnId <= 0L) {
                return;
            }
            autoStopPending.set(true);
            autoStopAfterTurn.updateAndGet(previous -> previous < 0L ? turnId : Math.max(previous, turnId));
            if (reason != null && !reason.isBlank()) {
                autoStopReason = reason;
            }
        }

        private void tryCompleteTurn(long turnId) {
            if (closed.get() || !turnsReadyForCompletion.contains(turnId)) {
                return;
            }
            AtomicLong pending = pendingSpeechCounts.get(turnId);
            if (pending != null && pending.get() > 0) {
                return;
            }
            if (!completedTurns.add(turnId)) {
                return;
            }
            turnsReadyForCompletion.remove(turnId);
            send("turn.completed", Map.of(
                    "sessionId", sessionId,
                    "turnId", turnId,
                    "cancelled", cancelledThroughTurn.get() >= turnId
            ));

            long stopAfter = autoStopAfterTurn.get();
            if (autoStopPending.get() && stopAfter > 0L && turnId >= stopAfter) {
                if (autoStopPending.compareAndSet(true, false) && !closed.get()) {
                    Map<String, Object> payload = new LinkedHashMap<>();
                    payload.put("sessionId", sessionId);
                    payload.put("forced", false);
                    payload.put("reason", autoStopReason == null ? "" : autoStopReason);
                    send("session.stopped", payload);
                    close();
                }
            }
        }

        @Override
        public void appendAudio(String base64Audio) {
            if (closed.get() || isBlank(base64Audio) || audioSession == null) {
                return;
            }
            audioSession.appendBase64Audio(base64Audio);
        }

        @Override
        public void commitAudio() {
            if (closed.get() || audioSession == null) {
                return;
            }
            audioSession.commit();
        }

        @Override
        public void cancelResponse() {
            if (closed.get()) {
                return;
            }
            long turnId = activeTurn.get();
            if (turnId > 0L) {
                cancelledThroughTurn.updateAndGet(previous -> Math.max(previous, turnId));
            }
            if (audioSession != null) {
                audioSession.cancelResponse();
            }
            send("response.cancelled", Map.of("sessionId", sessionId, "turnId", turnId));
        }

        @Override
        public void update(RealtimeSessionUpdateCommand command) {
            if (closed.get() || command == null) {
                return;
            }
            settings = settings.apply(command);
            Map<String, Object> payload = new LinkedHashMap<>();
            payload.put("sessionId", sessionId);
            payload.put("conversationModel", settings.conversationModel);
            payload.put("sttModel", settings.sttModel);
            payload.put("feedbackProvider", settings.feedbackProvider);
            payload.put("feedbackModel", settings.feedbackModel);
            payload.put("feedbackLanguage", settings.feedbackLanguage);
            payload.put("ttsVoice", settings.ttsVoice);
            send("session.updated", payload);
        }

        @Override
        public void stopSession(boolean forced, String reason) {
            if (closed.get()) {
                return;
            }
            if (forced) {
                forcedStopped.set(true);
            }
            if (audioSession != null) {
                try {
                    audioSession.cancelResponse();
                } catch (Exception ignored) {
                }
            }
            Map<String, Object> payload = new LinkedHashMap<>();
            payload.put("sessionId", sessionId);
            payload.put("forced", forced);
            payload.put("reason", reason == null ? "" : reason);
            send("session.stopped", payload);
            close();
        }

        @Override
        public void close() {
            if (!closed.compareAndSet(false, true)) {
                return;
            }
            if (audioSession != null) {
                audioSession.close();
            }
        }

        private void send(String event, Object payload) {
            try {
                boolean sent = sink.send(event, payload);
                if (!sent) {
                    close();
                }
            } catch (Exception ignored) {
                close();
            }
        }
    }
}
