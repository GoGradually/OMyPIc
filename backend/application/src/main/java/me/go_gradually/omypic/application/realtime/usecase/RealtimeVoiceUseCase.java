package me.go_gradually.omypic.application.realtime.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
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
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionState;

import java.time.Duration;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
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
            NextQuestion nextQuestion = requestNextQuestion(context, turnId);
            if (!isBlank(nextQuestion.getText())) {
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
        context.activeTurn.set(turnId);
        context.send("stt.final", Map.of("sessionId", context.sessionId, "turnId", turnId, "text", text));
        sessionUseCase.appendSegment(context.sessionId, text);
        Instant startedAt = Instant.now();
        asyncExecutor.execute(() -> processTurn(context, turnId, text, startedAt));
    }

    private void processTurn(RuntimeContext context, long turnId, String transcript, Instant startedAt) {
        RuntimeSettings settings = context.settings;
        try {
            if (context.closed.get() || context.forcedStopped.get() || isCancelled(context, turnId)) {
                return;
            }

            SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
            ModeType mode = sessionState.getMode();
            String feedbackApiKey = firstNonBlank(settings.feedbackApiKey, context.apiKey);

            FeedbackResult feedbackResult = FeedbackResult.skipped();
            if (mode != ModeType.MOCK_EXAM) {
                feedbackResult = feedbackUseCase.generateFeedback(feedbackApiKey, feedbackCommand(settings, context.sessionId, transcript));
                if (feedbackResult.isGenerated()) {
                    Feedback feedback = feedbackResult.getFeedback();
                    if (!context.closed.get() && !context.forcedStopped.get() && !isCancelled(context, turnId)) {
                        context.send("feedback.final", feedbackPayload(context.sessionId, turnId, feedback));
                        streamSpeech(context, turnId, toTtsText(feedback));
                    }
                } else {
                    context.send("feedback.skipped", Map.of("sessionId", context.sessionId, "turnId", turnId));
                }
            }

            NextQuestion nextQuestion = null;
            if (shouldPromptNextQuestion(mode, feedbackResult) && !context.closed.get() && !context.forcedStopped.get()) {
                nextQuestion = requestNextQuestion(context, turnId);
                if (!isBlank(nextQuestion.getText()) && !isCancelled(context, turnId)) {
                    streamSpeech(context, turnId, nextQuestion.getText());
                }
            }

            if (mode == ModeType.MOCK_EXAM && nextQuestion != null && nextQuestion.isMockExamCompleted() && !context.forcedStopped.get()) {
                FeedbackResult mockFinalFeedback = feedbackUseCase.generateMockExamFinalFeedback(
                        feedbackApiKey,
                        feedbackCommand(settings, context.sessionId, transcript)
                );
                if (mockFinalFeedback.isGenerated() && !isCancelled(context, turnId) && !context.closed.get() && !context.forcedStopped.get()) {
                    Feedback feedback = mockFinalFeedback.getFeedback();
                    context.send("feedback.final", feedbackPayload(context.sessionId, turnId, feedback));
                    streamSpeech(context, turnId, toTtsText(feedback));
                }
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

    private boolean shouldPromptNextQuestion(ModeType mode, FeedbackResult feedbackResult) {
        if (mode == ModeType.IMMEDIATE) {
            return true;
        }
        if (mode == ModeType.CONTINUOUS) {
            return !feedbackResult.isGenerated();
        }
        return mode == ModeType.MOCK_EXAM;
    }

    private NextQuestion requestNextQuestion(RuntimeContext context, long turnId) {
        String listId = resolveActiveQuestionListId(context.sessionId);
        NextQuestion nextQuestion = questionUseCase.nextQuestion(listId, context.sessionId);
        context.send("question.prompt", questionPayload(context.sessionId, turnId, nextQuestion));
        return nextQuestion;
    }

    private String resolveActiveQuestionListId(String sessionId) {
        SessionState state = sessionUseCase.getOrCreate(sessionId);
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

    private Map<String, Object> questionPayload(String sessionId, long turnId, NextQuestion question) {
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("sessionId", sessionId);
        payload.put("turnId", turnId);
        payload.put("questionId", question.getQuestionId());
        payload.put("text", question.getText());
        payload.put("group", question.getGroup());
        payload.put("skipped", question.isSkipped());
        payload.put("mockExamCompleted", question.isMockExamCompleted());
        payload.put("mockExamCompletionReason", question.getMockExamCompletionReason());
        return payload;
    }

    private Map<String, Object> feedbackPayload(String sessionId, long turnId, Feedback feedback) {
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("sessionId", sessionId);
        payload.put("turnId", turnId);
        payload.put("summary", feedback.getSummary());
        payload.put("correctionPoints", feedback.getCorrectionPoints());
        payload.put("exampleAnswer", feedback.getExampleAnswer());
        payload.put("rulebookEvidence", feedback.getRulebookEvidence());
        return payload;
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

    private boolean isCancelled(RuntimeContext context, long turnId) {
        return context.cancelledThroughTurn.get() >= turnId;
    }

    private String defaultMessage(String message) {
        return isBlank(message) ? "Unknown realtime error" : message;
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
        private final Map<Long, AtomicLong> pendingSpeechCounts = new ConcurrentHashMap<>();
        private final Set<Long> turnsReadyForCompletion = ConcurrentHashMap.newKeySet();
        private final Set<Long> completedTurns = ConcurrentHashMap.newKeySet();
        private final AtomicBoolean closed = new AtomicBoolean(false);
        private final AtomicBoolean forcedStopped = new AtomicBoolean(false);
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
