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
        validateStartCommand(command);
        sessionUseCase.getOrCreate(command.getSessionId());
        RuntimeSettings settings = resolveRuntimeSettings(command);
        RuntimeContext context = new RuntimeContext(command.getSessionId(), command.getApiKey(), sink, settings);
        context.audioSession = openAudioSession(command, settings, context);
        context.send("connection.ready", Map.of("sessionId", context.sessionId));
        asyncExecutor.execute(() -> sendInitialQuestion(context));
        return context;
    }

    private void validateStartCommand(RealtimeStartCommand command) {
        if (command == null || isBlank(command.getSessionId()) || isBlank(command.getApiKey())) {
            throw new IllegalArgumentException("sessionId and apiKey are required");
        }
    }

    private RuntimeSettings resolveRuntimeSettings(RealtimeStartCommand command) {
        RuntimeSettings defaults = RuntimeSettings.defaults(realtimePolicy);
        String conversationModel = firstNonBlank(command.getConversationModel(), defaults.conversationModel());
        String sttModel = firstNonBlank(command.getSttModel(), defaults.sttModel());
        return defaults.withRealtimeModels(conversationModel, sttModel);
    }

    private RealtimeAudioSession openAudioSession(RealtimeStartCommand command,
                                                  RuntimeSettings settings,
                                                  RuntimeContext context) {
        RealtimeAudioOpenCommand openCommand = new RealtimeAudioOpenCommand(
                command.getApiKey(), settings.conversationModel(), settings.sttModel()
        );
        return realtimeAudioGateway.open(openCommand, createAudioListener(context));
    }

    private RealtimeAudioEventListener createAudioListener(RuntimeContext context) {
        return new GatewayAudioListener(context);
    }

    private void handlePartialTranscript(RuntimeContext context, String delta) {
        if (isContextInactive(context) || isBlank(delta)) {
            return;
        }
        context.send("stt.partial", Map.of("sessionId", context.sessionId, "text", delta));
    }

    private void handleFinalTranscriptEvent(RuntimeContext context, String text) {
        if (isContextInactive(context) || isBlank(text)) {
            return;
        }
        handleFinalTranscript(context, text);
    }

    private void handleAssistantAudioChunk(RuntimeContext context, long turnId, String base64Audio) {
        if (isTurnInactive(context, turnId) || isBlank(base64Audio)) {
            return;
        }
        context.send("tts.chunk", Map.of("sessionId", context.sessionId, "turnId", turnId, "audio", base64Audio));
    }

    private void handleAssistantAudioFailed(RuntimeContext context, long turnId, String message) {
        context.completeSpeech(turnId);
        metrics.incrementTtsError();
        if (context.closed.get()) {
            return;
        }
        context.send("tts.error", ttsErrorPayload(context, turnId, defaultMessage(message)));
    }

    private void handleRealtimeError(RuntimeContext context, String message) {
        if (context.closed.get()) {
            return;
        }
        context.send("error", Map.of("sessionId", context.sessionId, "message", defaultMessage(message)));
    }

    private void sendInitialQuestion(RuntimeContext context) {
        if (isContextInactive(context)) {
            return;
        }
        long turnId = context.nextTurnId();
        try {
            askInitialQuestion(context, turnId);
        } catch (Exception e) {
            context.send("error", Map.of("sessionId", context.sessionId, "message", defaultMessage(e.getMessage())));
        } finally {
            context.markTurnProcessingDone(turnId);
        }
    }

    private void askInitialQuestion(RuntimeContext context, long turnId) {
        SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
        NextQuestion nextQuestion = requestNextQuestion(context, turnId, sessionState);
        if (isQuestionExhausted(nextQuestion)) {
            context.requestAutoStop(turnId, SessionStopReason.QUESTION_EXHAUSTED.code());
            return;
        }
        streamSpeech(context, turnId, nextQuestion.getText());
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
        if (isTurnInactive(context, turnId)) {
            return;
        }
        runTurnProcessing(context, turnId, turnInput, startedAt, settings);
    }

    private void runTurnProcessing(RuntimeContext context,
                                   long turnId,
                                   TurnInput turnInput,
                                   Instant startedAt,
                                   RuntimeSettings settings) {
        try {
            executeTurn(context, turnId, turnInput, settings);
        } catch (Exception e) {
            sendTurnError(context, turnId, e);
        } finally {
            finishTurn(context, turnId, startedAt);
        }
    }

    private void executeTurn(RuntimeContext context, long turnId, TurnInput turnInput, RuntimeSettings settings) {
        TurnState state = prepareTurnState(context, turnId, turnInput, settings);
        emitMainFeedback(context, turnId, settings, state);
        emitResidualFeedback(context, turnId, settings, state);
        streamNextQuestion(context, turnId, state.questionSelection());
        requestAutoStopIfNeeded(context, turnId, state.questionSelection().nextAction());
    }

    private void finishTurn(RuntimeContext context, long turnId, Instant startedAt) {
        context.markTurnProcessingDone(turnId);
        metrics.recordRealtimeTurnLatency(Duration.between(startedAt, Instant.now()));
    }

    private TurnState prepareTurnState(RuntimeContext context,
                                       long turnId,
                                       TurnInput turnInput,
                                       RuntimeSettings settings) {
        SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
        ModeType mode = sessionState.getMode();
        sessionState.setFeedbackLanguage(FeedbackLanguage.of(settings.feedbackLanguage));
        String feedbackApiKey = firstNonBlank(settings.feedbackApiKey, context.apiKey);
        QuestionSelection selection = selectQuestionForTurn(context, turnId, sessionState, mode);
        FeedbackPlan feedbackPlan = resolveFeedbackPlan(context, turnId, turnInput, sessionState, mode, selection);
        return new TurnState(sessionState, mode, feedbackApiKey, selection, feedbackPlan);
    }

    private QuestionSelection selectQuestionForTurn(RuntimeContext context,
                                                    long turnId,
                                                    SessionState sessionState,
                                                    ModeType mode) {
        if (!shouldPromptNextQuestion(mode) || isContextInactive(context)) {
            return QuestionSelection.none();
        }
        NextQuestion nextQuestion = requestNextQuestion(context, turnId, sessionState);
        boolean exhausted = isQuestionExhausted(nextQuestion);
        SessionFlowPolicy.SessionAction nextAction = SessionFlowPolicy.decideAfterQuestionSelection(exhausted);
        return new QuestionSelection(nextQuestion, exhausted, nextAction);
    }

    private FeedbackPlan resolveFeedbackPlan(RuntimeContext context,
                                             long turnId,
                                             TurnInput turnInput,
                                             SessionState sessionState,
                                             ModeType mode,
                                             QuestionSelection selection) {
        if (mode == ModeType.IMMEDIATE) {
            return new FeedbackPlan(List.of(turnInput), TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE);
        }
        if (mode != ModeType.CONTINUOUS) {
            return new FeedbackPlan(List.of(), TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE);
        }
        return continuousFeedbackPlan(context, turnId, turnInput, sessionState, selection);
    }

    private FeedbackPlan continuousFeedbackPlan(RuntimeContext context,
                                                long turnId,
                                                TurnInput turnInput,
                                                SessionState sessionState,
                                                QuestionSelection selection) {
        context.appendContinuousTurn(turnInput);
        boolean completedGroup = isContinuousGroupCompleted(turnInput, selection.nextQuestion(), selection.questionExhausted());
        TurnBatchingPolicy.BatchDecision decision = sessionState.decideFeedbackBatchOnTurn(completedGroup);
        if (!decision.emitFeedback()) {
            sendFeedbackSkipped(context, turnId, decision.reason());
            return new FeedbackPlan(List.of(), decision.reason());
        }
        return new FeedbackPlan(context.pollAllContinuousTurns(), decision.reason());
    }

    private void sendFeedbackSkipped(RuntimeContext context, long turnId, TurnBatchingPolicy.BatchReason reason) {
        context.send("feedback.skipped", Map.of(
                "sessionId", context.sessionId,
                "turnId", turnId,
                "reason", reason.name()
        ));
    }

    private void emitMainFeedback(RuntimeContext context, long turnId, RuntimeSettings settings, TurnState state) {
        if (state.feedbackPlan().inputs().isEmpty()) {
            return;
        }
        List<FeedbackItem> items = generateFeedbackItems(settings, context.sessionId, state.feedbackApiKey(), state.feedbackPlan().inputs());
        if (isTurnInactive(context, turnId)) {
            return;
        }
        sendFeedbackFinal(context, turnId, state, state.feedbackPlan().reason(), false, items);
        streamFeedbackSpeech(context, turnId, state.mode(), items);
    }

    private void emitResidualFeedback(RuntimeContext context, long turnId, RuntimeSettings settings, TurnState state) {
        if (!shouldEmitResidualFeedback(state)) {
            return;
        }
        List<TurnInput> turns = context.pollAllContinuousTurns();
        if (turns.isEmpty()) {
            return;
        }
        List<FeedbackItem> items = generateFeedbackItems(settings, context.sessionId, state.feedbackApiKey(), turns);
        if (isTurnInactive(context, turnId)) {
            return;
        }
        sendFeedbackFinal(context, turnId, state, TurnBatchingPolicy.BatchReason.EXHAUSTED_WITH_REMAINDER, true, items);
        streamFeedbackSpeech(context, turnId, state.mode(), items);
    }

    private boolean shouldEmitResidualFeedback(TurnState state) {
        return state.sessionState().shouldGenerateResidualContinuousFeedback(
                state.questionSelection().questionExhausted(),
                !state.feedbackPlan().inputs().isEmpty()
        );
    }

    private void sendFeedbackFinal(RuntimeContext context,
                                   long turnId,
                                   TurnState state,
                                   TurnBatchingPolicy.BatchReason reason,
                                   boolean residual,
                                   List<FeedbackItem> items) {
        context.send("feedback.final", feedbackPayload(
                context.sessionId,
                turnId,
                state.mode(),
                feedbackBatchSize(state),
                reason,
                residual,
                items,
                state.questionSelection().nextAction()
        ));
    }

    private int feedbackBatchSize(TurnState state) {
        if (state.mode() == ModeType.CONTINUOUS) {
            return state.sessionState().getContinuousBatchSize();
        }
        return 1;
    }

    private void streamFeedbackSpeech(RuntimeContext context, long turnId, ModeType mode, List<FeedbackItem> items) {
        if (mode != ModeType.CONTINUOUS) {
            streamSpeech(context, turnId, toTtsText(items));
        }
    }

    private void streamNextQuestion(RuntimeContext context, long turnId, QuestionSelection selection) {
        NextQuestion nextQuestion = selection.nextQuestion();
        if (nextQuestion == null || selection.questionExhausted()) {
            return;
        }
        streamSpeech(context, turnId, nextQuestion.getText());
    }

    private void requestAutoStopIfNeeded(RuntimeContext context, long turnId, SessionFlowPolicy.SessionAction nextAction) {
        if (nextAction.type() == SessionFlowPolicy.NextActionType.AUTO_STOP) {
            context.requestAutoStop(turnId, nextAction.reason().code());
        }
    }

    private void sendTurnError(RuntimeContext context, long turnId, Exception e) {
        context.send("error", Map.of(
                "sessionId", context.sessionId,
                "turnId", turnId,
                "message", defaultMessage(e.getMessage())
        ));
    }

    private boolean shouldPromptNextQuestion(ModeType mode) {
        return mode == ModeType.IMMEDIATE || mode == ModeType.CONTINUOUS;
    }

    private boolean isQuestionExhausted(NextQuestion nextQuestion) {
        return nextQuestion == null || nextQuestion.isSkipped();
    }

    private NextQuestion requestNextQuestion(RuntimeContext context, long turnId, SessionState sessionState) {
        NextQuestion nextQuestion = questionUseCase.nextQuestion(context.sessionId);
        context.updateCurrentQuestion(nextQuestion);
        context.send("question.prompt", questionPayload(context.sessionId, turnId, sessionState.getMode(), nextQuestion));
        return nextQuestion;
    }

    private boolean isContinuousGroupCompleted(TurnInput turnInput, NextQuestion nextQuestion, boolean exhausted) {
        if (turnInput == null || isBlank(turnInput.questionGroupId())) {
            return false;
        }
        if (exhausted || nextQuestion == null || nextQuestion.isSkipped()) {
            return true;
        }
        String nextGroupId = nextQuestion.getGroupId();
        if (isBlank(nextGroupId)) {
            return false;
        }
        return !turnInput.questionGroupId().equals(nextGroupId);
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
        payload.put("selection", selectionPayload(mode, exhausted, question));
        return payload;
    }

    private Map<String, Object> selectionPayload(ModeType mode, boolean exhausted, NextQuestion question) {
        Map<String, Object> selection = new LinkedHashMap<>();
        selection.put("mode", mode == null ? ModeType.IMMEDIATE.name() : mode.name());
        selection.put("exhausted", exhausted);
        if (exhausted) {
            selection.put("reason", resolveExhaustedReason(question));
        }
        return selection;
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
                                                int groupBatchSize,
                                                TurnBatchingPolicy.BatchReason reason,
                                                boolean residual,
                                                List<FeedbackItem> items,
                                                SessionFlowPolicy.SessionAction nextAction) {
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("sessionId", sessionId);
        payload.put("turnId", turnId);
        payload.put("policy", feedbackPolicyPayload(mode, groupBatchSize, reason));
        payload.put("batch", feedbackBatchPayload(items, residual));
        payload.put("nextAction", nextActionPayload(nextAction));
        return payload;
    }

    private Map<String, Object> feedbackPolicyPayload(ModeType mode, int groupBatchSize, TurnBatchingPolicy.BatchReason reason) {
        Map<String, Object> policy = new LinkedHashMap<>();
        policy.put("mode", mode == null ? ModeType.IMMEDIATE.name() : mode.name());
        policy.put("groupBatchSize", Math.max(1, groupBatchSize));
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
        return SessionStopReason.QUESTION_EXHAUSTED.code();
    }

    private void streamSpeech(RuntimeContext context, long turnId, String speechText) {
        if (!canStreamSpeech(context, turnId, speechText)) {
            return;
        }
        RealtimeAudioSession audioSession = context.audioSession;
        if (audioSession == null) {
            sendMissingAudioSession(context, turnId);
            return;
        }
        context.registerSpeech(turnId);
        speakSpeech(context, turnId, speechText, audioSession);
    }

    private boolean canStreamSpeech(RuntimeContext context, long turnId, String speechText) {
        return !isBlank(speechText) && !isTurnInactive(context, turnId);
    }

    private void sendMissingAudioSession(RuntimeContext context, long turnId) {
        metrics.incrementTtsError();
        context.send("tts.error", ttsErrorPayload(context, turnId, "Realtime audio session is not available"));
    }

    private Map<String, Object> ttsErrorPayload(RuntimeContext context, long turnId, String message) {
        return Map.of("sessionId", context.sessionId, "turnId", turnId, "message", message);
    }

    private void speakSpeech(RuntimeContext context, long turnId, String speechText, RealtimeAudioSession audioSession) {
        try {
            audioSession.speakText(turnId, speechText, context.settings.ttsVoice);
        } catch (Exception e) {
            context.completeSpeech(turnId);
            metrics.incrementTtsError();
            context.send("tts.error", ttsErrorPayload(context, turnId, defaultMessage(e.getMessage())));
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
        appendSummary(sb, feedback);
        appendCorrectionPoints(sb, feedback);
        appendExampleAnswer(sb, feedback);
        return fallbackIfEmpty(sb.toString().trim());
    }

    private String toTtsText(List<FeedbackItem> items) {
        if (items == null || items.isEmpty()) {
            return "No feedback available.";
        }
        if (items.size() == 1) {
            return toTtsText(items.get(0).feedback());
        }
        return buildMultiFeedbackTts(items);
    }

    private String buildMultiFeedbackTts(List<FeedbackItem> items) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < items.size(); i++) {
            appendFeedbackItem(sb, items.get(i), i + 1);
        }
        return sb.toString().trim();
    }

    private void appendSummary(StringBuilder sb, Feedback feedback) {
        if (!isBlank(feedback.getSummary())) {
            sb.append(feedback.getSummary()).append('\n');
        }
    }

    private void appendCorrectionPoints(StringBuilder sb, Feedback feedback) {
        if (!feedback.getCorrectionPoints().isEmpty()) {
            sb.append(String.join("\n", feedback.getCorrectionPoints())).append('\n');
        }
    }

    private void appendExampleAnswer(StringBuilder sb, Feedback feedback) {
        if (!isBlank(feedback.getExampleAnswer())) {
            sb.append(feedback.getExampleAnswer());
        }
    }

    private String fallbackIfEmpty(String text) {
        return text.isEmpty() ? "No feedback available." : text;
    }

    private void appendFeedbackItem(StringBuilder sb, FeedbackItem item, int index) {
        sb.append("Feedback ").append(index).append('\n');
        if (!isBlank(item.questionText())) {
            sb.append(item.questionText()).append('\n');
        }
        sb.append(toTtsText(item.feedback())).append('\n');
    }

    private boolean isCancelled(RuntimeContext context, long turnId) {
        return context.cancelledThroughTurn.get() >= turnId;
    }

    private boolean isContextInactive(RuntimeContext context) {
        return context.closed.get() || context.forcedStopped.get();
    }

    private boolean isTurnInactive(RuntimeContext context, long turnId) {
        return isContextInactive(context) || isCancelled(context, turnId);
    }

    private String defaultMessage(String message) {
        return isBlank(message) ? "Unknown realtime error" : message;
    }

    private record QuestionSelection(NextQuestion nextQuestion,
                                     boolean questionExhausted,
                                     SessionFlowPolicy.SessionAction nextAction) {
        private static QuestionSelection none() {
            return new QuestionSelection(null, false, SessionFlowPolicy.SessionAction.askNext());
        }
    }

    private record FeedbackPlan(List<TurnInput> inputs, TurnBatchingPolicy.BatchReason reason) {
    }

    private record TurnState(SessionState sessionState,
                             ModeType mode,
                             String feedbackApiKey,
                             QuestionSelection questionSelection,
                             FeedbackPlan feedbackPlan) {
    }

    private final class GatewayAudioListener implements RealtimeAudioEventListener {
        private final RuntimeContext context;

        private GatewayAudioListener(RuntimeContext context) {
            this.context = context;
        }

        @Override
        public void onPartialTranscript(String delta) {
            handlePartialTranscript(context, delta);
        }

        @Override
        public void onFinalTranscript(String text) {
            handleFinalTranscriptEvent(context, text);
        }

        @Override
        public void onAssistantAudioChunk(long turnId, String base64Audio) {
            handleAssistantAudioChunk(context, turnId, base64Audio);
        }

        @Override
        public void onAssistantAudioCompleted(long turnId) {
            context.completeSpeech(turnId);
        }

        @Override
        public void onAssistantAudioFailed(long turnId, String message) {
            handleAssistantAudioFailed(context, turnId, message);
        }

        @Override
        public void onError(String message) {
            handleRealtimeError(context, message);
        }
    }

    private record TurnInput(String questionId,
                             String questionText,
                             String questionGroupId,
                             QuestionGroup questionGroup,
                             String answerText) {
        private static TurnInput fromQuestion(NextQuestion question) {
            if (question == null) {
                return empty();
            }
            return new TurnInput(
                    safe(question.getQuestionId()),
                    safe(question.getText()),
                    safe(question.getGroupId()),
                    QuestionGroup.fromNullable(question.getGroup()),
                    ""
            );
        }

        private static TurnInput empty() {
            return new TurnInput("", "", "", null, "");
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
                    question.questionGroupId(),
                    question.questionGroup(),
                    answerText == null ? "" : answerText.trim()
            );
        }

        private void appendContinuousTurn(TurnInput turnInput) {
            continuousTurns.addLast(turnInput);
        }

        private List<TurnInput> pollAllContinuousTurns() {
            List<TurnInput> turns = new ArrayList<>();
            TurnInput item;
            while ((item = continuousTurns.pollFirst()) != null) {
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
            if (!canCompleteTurn(turnId)) {
                return;
            }
            publishTurnCompletion(turnId);
            maybeAutoStop(turnId);
        }

        private boolean canCompleteTurn(long turnId) {
            if (closed.get() || !turnsReadyForCompletion.contains(turnId)) {
                return false;
            }
            AtomicLong pending = pendingSpeechCounts.get(turnId);
            if (pending != null && pending.get() > 0) {
                return false;
            }
            return completedTurns.add(turnId);
        }

        private void publishTurnCompletion(long turnId) {
            turnsReadyForCompletion.remove(turnId);
            send("turn.completed", Map.of(
                    "sessionId", sessionId,
                    "turnId", turnId,
                    "cancelled", cancelledThroughTurn.get() >= turnId
            ));
        }

        private void maybeAutoStop(long turnId) {
            long stopAfter = autoStopAfterTurn.get();
            if (!shouldAutoStop(turnId, stopAfter)) {
                return;
            }
            send("session.stopped", autoStopPayload());
            close();
        }

        private boolean shouldAutoStop(long turnId, long stopAfter) {
            if (!autoStopPending.get() || stopAfter <= 0L || turnId < stopAfter) {
                return false;
            }
            return autoStopPending.compareAndSet(true, false) && !closed.get();
        }

        private Map<String, Object> autoStopPayload() {
            Map<String, Object> payload = new LinkedHashMap<>();
            payload.put("sessionId", sessionId);
            payload.put("forced", false);
            payload.put("reason", autoStopReason == null ? "" : autoStopReason);
            return payload;
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
            send("session.updated", sessionUpdatedPayload());
        }

        private Map<String, Object> sessionUpdatedPayload() {
            Map<String, Object> payload = new LinkedHashMap<>();
            payload.put("sessionId", sessionId);
            payload.put("conversationModel", settings.conversationModel);
            payload.put("sttModel", settings.sttModel);
            payload.put("feedbackProvider", settings.feedbackProvider);
            payload.put("feedbackModel", settings.feedbackModel);
            payload.put("feedbackLanguage", settings.feedbackLanguage);
            payload.put("ttsVoice", settings.ttsVoice);
            return payload;
        }

        @Override
        public void stopSession(boolean forced, String reason) {
            if (closed.get()) {
                return;
            }
            markForcedStop(forced);
            cancelAudioResponse();
            send("session.stopped", stopPayload(forced, reason));
            close();
        }

        private void markForcedStop(boolean forced) {
            if (forced) {
                forcedStopped.set(true);
            }
        }

        private void cancelAudioResponse() {
            if (audioSession == null) {
                return;
            }
            try {
                audioSession.cancelResponse();
            } catch (Exception ignored) {
            }
        }

        private Map<String, Object> stopPayload(boolean forced, String reason) {
            Map<String, Object> payload = new LinkedHashMap<>();
            payload.put("sessionId", sessionId);
            payload.put("forced", forced);
            payload.put("reason", reason == null ? "" : reason);
            return payload;
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
