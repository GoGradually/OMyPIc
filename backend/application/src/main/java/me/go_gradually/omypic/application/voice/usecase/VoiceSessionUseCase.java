package me.go_gradually.omypic.application.voice.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.application.voice.model.VoiceAudioChunkCommand;
import me.go_gradually.omypic.application.voice.model.VoiceEventSink;
import me.go_gradually.omypic.application.voice.model.VoiceSessionOpenCommand;
import me.go_gradually.omypic.application.voice.model.VoiceSessionStopCommand;
import me.go_gradually.omypic.application.voice.policy.VoicePolicy;
import me.go_gradually.omypic.application.voice.port.TtsGateway;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionFlowPolicy;
import me.go_gradually.omypic.domain.session.SessionState;
import me.go_gradually.omypic.domain.session.SessionStopReason;
import me.go_gradually.omypic.domain.session.TurnBatchingPolicy;

import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Base64;
import java.util.Deque;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedDeque;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

public class VoiceSessionUseCase {
    private static final String FEEDBACK_PROVIDER = "openai";
    private static final int DEFAULT_SAMPLE_RATE = 16000;
    private static final int MAX_RULEBOOK_DOCUMENTS_PER_TURN = 2;
    private final SttUseCase sttUseCase;
    private final FeedbackUseCase feedbackUseCase;
    private final SessionUseCase sessionUseCase;
    private final QuestionUseCase questionUseCase;
    private final TtsGateway ttsGateway;
    private final AsyncExecutor asyncExecutor;
    private final VoicePolicy voicePolicy;
    private final MetricsPort metrics;
    private final Map<String, RuntimeContext> contextByVoiceSessionId = new ConcurrentHashMap<>();

    public VoiceSessionUseCase(SttUseCase sttUseCase,
                               FeedbackUseCase feedbackUseCase,
                               SessionUseCase sessionUseCase,
                               QuestionUseCase questionUseCase,
                               TtsGateway ttsGateway,
                               AsyncExecutor asyncExecutor,
                               VoicePolicy voicePolicy,
                               MetricsPort metrics) {
        this.sttUseCase = sttUseCase;
        this.feedbackUseCase = feedbackUseCase;
        this.sessionUseCase = sessionUseCase;
        this.questionUseCase = questionUseCase;
        this.ttsGateway = ttsGateway;
        this.asyncExecutor = asyncExecutor;
        this.voicePolicy = voicePolicy;
        this.metrics = metrics;
    }

    public String open(VoiceSessionOpenCommand command) {
        if (command == null || isBlank(command.getSessionId()) || isBlank(command.getApiKey())) {
            throw new IllegalArgumentException("sessionId and apiKey are required");
        }
        sessionUseCase.getOrCreate(command.getSessionId());
        RuntimeSettings settings = RuntimeSettings.resolve(command, voicePolicy);
        String voiceSessionId = UUID.randomUUID().toString();
        RuntimeContext context = new RuntimeContext(
                voiceSessionId,
                command.getSessionId(),
                command.getApiKey(),
                settings
        );
        contextByVoiceSessionId.put(voiceSessionId, context);
        return voiceSessionId;
    }

    public void registerSink(String voiceSessionId, VoiceEventSink sink) {
        RuntimeContext context = requireContext(voiceSessionId);
        context.addSink(sink);
        if (!context.markInitialized()) {
            return;
        }
        asyncExecutor.execute(() -> initializeSession(context));
    }

    public void unregisterSink(String voiceSessionId, VoiceEventSink sink) {
        RuntimeContext context = contextByVoiceSessionId.get(voiceSessionId);
        if (context == null) {
            return;
        }
        context.removeSink(sink);
    }

    public void appendAudio(VoiceAudioChunkCommand command) {
        validateAudioCommand(command);
        RuntimeContext context = requireContext(command.getVoiceSessionId());
        if (context.isInactive()) {
            return;
        }
        byte[] pcm16Bytes = decodePcm16(command.getPcm16Base64());
        if (pcm16Bytes.length == 0) {
            return;
        }
        int sampleRate = command.getSampleRate() == null ? DEFAULT_SAMPLE_RATE : command.getSampleRate();
        context.appendAudio(pcm16Bytes, sampleRate);
        flushTurn(context);
    }

    public void stop(VoiceSessionStopCommand command) {
        if (command == null || isBlank(command.getVoiceSessionId())) {
            throw new IllegalArgumentException("voiceSessionId is required");
        }
        RuntimeContext context = contextByVoiceSessionId.get(command.getVoiceSessionId());
        if (context == null) {
            return;
        }
        stopInternal(context, command.isForced(), normalizeStopReason(command.getReason(), "user_stop"));
    }

    private void validateAudioCommand(VoiceAudioChunkCommand command) {
        if (command == null || isBlank(command.getVoiceSessionId())) {
            throw new IllegalArgumentException("voiceSessionId is required");
        }
        if (isBlank(command.getPcm16Base64())) {
            throw new IllegalArgumentException("pcm16Base64 is required");
        }
    }

    private RuntimeContext requireContext(String voiceSessionId) {
        RuntimeContext context = contextByVoiceSessionId.get(voiceSessionId);
        if (context == null) {
            throw new IllegalArgumentException("Unknown voice session: " + voiceSessionId);
        }
        return context;
    }

    private byte[] decodePcm16(String base64Pcm16) {
        try {
            return Base64.getDecoder().decode(base64Pcm16);
        } catch (IllegalArgumentException e) {
            throw new IllegalArgumentException("Invalid base64 audio chunk", e);
        }
    }

    private void initializeSession(RuntimeContext context) {
        if (context.isInactive()) {
            return;
        }
        context.emit("session.ready", Map.of(
                "sessionId", context.sessionId,
                "voiceSessionId", context.voiceSessionId
        ));
        try {
            sendQuestionPrompt(context);
        } catch (Exception e) {
            context.emit("error", errorPayload(context.sessionId, 0L, defaultMessage(e.getMessage())));
            stopInternal(context, false, "initialization_failed");
        }
    }

    private void sendQuestionPrompt(RuntimeContext context) {
        SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
        NextQuestion nextQuestion = questionUseCase.nextQuestion(context.sessionId);
        boolean exhausted = isQuestionExhausted(nextQuestion);
        long turnId = exhausted ? 0L : context.nextTurnId();
        ModeType mode = sessionState.getMode();
        context.emit("question.prompt", questionPayload(context.sessionId, turnId, mode, nextQuestion));
        if (exhausted) {
            stopInternal(context, false, SessionStopReason.QUESTION_EXHAUSTED.code());
            return;
        }
        context.setCurrentQuestion(nextQuestion, turnId);
        emitSpeech(context, turnId, "question", nextQuestion.getText());
    }

    private void flushTurn(RuntimeContext context) {
        if (context.isInactive()) {
            return;
        }
        if (!context.startTurnProcessing()) {
            return;
        }
        AudioSnapshot snapshot = context.consumeAudioSnapshot();
        if (snapshot == null || snapshot.pcm16().length == 0 || snapshot.question() == null) {
            completeTurnProcessing(context);
            return;
        }
        asyncExecutor.execute(() -> processTurn(context, snapshot));
    }

    private void processTurn(RuntimeContext context, AudioSnapshot snapshot) {
        Instant startedAt = Instant.now();
        long turnId = snapshot.turnId();
        try {
            context.emit("turn.flush", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "bytes", snapshot.pcm16().length,
                    "sampleRate", snapshot.sampleRate(),
                    "durationMs", audioDurationMs(snapshot.pcm16().length, snapshot.sampleRate())
            ));
            String transcript = transcribe(snapshot, context);
            if (isBlank(transcript)) {
                context.emit("stt.skipped", Map.of(
                        "sessionId", context.sessionId,
                        "turnId", turnId,
                        "reason", "empty_transcript"
                ));
                context.emit("error", errorPayload(context.sessionId, turnId, "음성이 인식되지 않았습니다."));
                return;
            }
            context.emit("stt.final", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "text", transcript
            ));
            processTurnFlow(context, snapshot.question(), transcript, turnId);
            metrics.recordVoiceTurnLatency(Duration.between(startedAt, Instant.now()));
        } catch (Exception e) {
            context.emit("error", errorPayload(context.sessionId, turnId, defaultMessage(e.getMessage())));
        } finally {
            completeTurnProcessing(context);
        }
    }

    private void processTurnFlow(RuntimeContext context,
                                 QuestionSnapshot currentQuestion,
                                 String answerText,
                                 long turnId) {
        SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
        ModeType mode = sessionState.getMode();
        sessionState.setFeedbackLanguage(FeedbackLanguage.of(context.settings.feedbackLanguage()));
        TurnInput turnInput = toTurnInput(currentQuestion, answerText);

        NextQuestion nextQuestion = questionUseCase.nextQuestion(context.sessionId);
        boolean exhausted = isQuestionExhausted(nextQuestion);
        SessionFlowPolicy.SessionAction nextAction = SessionFlowPolicy.decideAfterQuestionSelection(exhausted);

        FeedbackPlan feedbackPlan = resolveFeedbackPlan(context, turnInput, sessionState, mode, nextQuestion, exhausted);
        if (mode == ModeType.IMMEDIATE) {
            boolean emittedFeedback = emitMainFeedback(context, turnId, mode, feedbackPlan, nextAction);
            emitResidualFeedback(context, turnId, mode, sessionState, nextAction, exhausted, emittedFeedback);
            long nextQuestionTurnId = emitQuestionPromptAndTrack(context, mode, nextQuestion, exhausted, turnId);
            if (nextQuestionTurnId > 0) {
                emitSpeech(context, nextQuestionTurnId, "question", nextQuestion.getText());
            }
        } else {
            long nextQuestionTurnId = emitQuestionPromptAndTrack(context, mode, nextQuestion, exhausted, turnId);
            if (nextQuestionTurnId > 0) {
                emitSpeech(context, nextQuestionTurnId, "question", nextQuestion.getText());
            }
            boolean emittedFeedback = emitMainFeedback(context, turnId, mode, feedbackPlan, nextAction);
            emitResidualFeedback(context, turnId, mode, sessionState, nextAction, exhausted, emittedFeedback);
        }

        if (nextAction.type() == SessionFlowPolicy.NextActionType.AUTO_STOP) {
            stopInternal(context, false, nextAction.reason().code());
        }
    }

    private long emitQuestionPromptAndTrack(RuntimeContext context,
                                            ModeType mode,
                                            NextQuestion nextQuestion,
                                            boolean exhausted,
                                            long fallbackTurnId) {
        if (exhausted) {
            context.emit("question.prompt", questionPayload(context.sessionId, fallbackTurnId, mode, nextQuestion));
            context.clearCurrentQuestion();
            return 0L;
        }
        long nextTurnId = context.nextTurnId();
        context.setCurrentQuestion(nextQuestion, nextTurnId);
        context.emit("question.prompt", questionPayload(context.sessionId, nextTurnId, mode, nextQuestion));
        return nextTurnId;
    }

    private FeedbackPlan resolveFeedbackPlan(RuntimeContext context,
                                             TurnInput turnInput,
                                             SessionState sessionState,
                                             ModeType mode,
                                             NextQuestion nextQuestion,
                                             boolean exhausted) {
        if (mode == ModeType.IMMEDIATE) {
            return new FeedbackPlan(List.of(turnInput), TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE);
        }
        if (mode != ModeType.CONTINUOUS) {
            return new FeedbackPlan(List.of(), TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE);
        }
        context.appendContinuousTurn(turnInput);
        boolean completedGroup = isContinuousGroupCompleted(turnInput, nextQuestion, exhausted);
        TurnBatchingPolicy.BatchDecision decision = sessionState.decideFeedbackBatchOnTurn(completedGroup);
        if (!decision.emitFeedback()) {
            context.emit("feedback.skipped", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", context.currentTurnId(),
                    "reason", decision.reason().name()
            ));
            return new FeedbackPlan(List.of(), decision.reason());
        }
        return new FeedbackPlan(context.pollAllContinuousTurns(), decision.reason());
    }

    private boolean emitMainFeedback(RuntimeContext context,
                                     long turnId,
                                     ModeType mode,
                                     FeedbackPlan plan,
                                     SessionFlowPolicy.SessionAction nextAction) {
        if (plan.inputs().isEmpty()) {
            return false;
        }
        List<FeedbackItem> items = generateFeedbackItems(context, plan.inputs());
        context.emit("feedback.final", feedbackPayload(
                context.sessionId,
                turnId,
                mode,
                groupBatchSize(mode, sessionUseCase.getOrCreate(context.sessionId)),
                plan.reason(),
                false,
                items,
                nextAction
        ));
        if (mode != ModeType.CONTINUOUS) {
            emitSpeech(context, turnId, "feedback", toTtsText(items));
        }
        return true;
    }

    private void emitResidualFeedback(RuntimeContext context,
                                      long turnId,
                                      ModeType mode,
                                      SessionState sessionState,
                                      SessionFlowPolicy.SessionAction nextAction,
                                      boolean exhausted,
                                      boolean emittedFeedback) {
        if (!sessionState.shouldGenerateResidualContinuousFeedback(exhausted, emittedFeedback)) {
            return;
        }
        List<TurnInput> remainder = context.pollAllContinuousTurns();
        if (remainder.isEmpty()) {
            return;
        }
        List<FeedbackItem> items = generateFeedbackItems(context, remainder);
        context.emit("feedback.final", feedbackPayload(
                context.sessionId,
                turnId,
                mode,
                groupBatchSize(mode, sessionState),
                TurnBatchingPolicy.BatchReason.EXHAUSTED_WITH_REMAINDER,
                true,
                items,
                nextAction
        ));
    }

    private int groupBatchSize(ModeType mode, SessionState sessionState) {
        if (mode == ModeType.CONTINUOUS) {
            return sessionState.getContinuousBatchSize();
        }
        return 1;
    }

    private List<FeedbackItem> generateFeedbackItems(RuntimeContext context, List<TurnInput> inputs) {
        List<FeedbackItem> items = new ArrayList<>();
        for (TurnInput input : inputs) {
            Feedback feedback = feedbackUseCase.generateFeedbackForTurn(
                    context.apiKey,
                    feedbackCommand(context, input.answerText()),
                    input.questionText(),
                    input.questionGroup(),
                    input.answerText(),
                    MAX_RULEBOOK_DOCUMENTS_PER_TURN
            );
            items.add(new FeedbackItem(
                    input.questionId(),
                    input.questionText(),
                    input.questionGroup(),
                    input.answerText(),
                    feedback
            ));
        }
        return items;
    }

    private FeedbackCommand feedbackCommand(RuntimeContext context, String text) {
        FeedbackCommand command = new FeedbackCommand();
        command.setSessionId(context.sessionId);
        command.setText(text);
        command.setProvider(FEEDBACK_PROVIDER);
        command.setModel(context.settings.feedbackModel());
        command.setFeedbackLanguage(context.settings.feedbackLanguage());
        return command;
    }

    private TurnInput toTurnInput(QuestionSnapshot question, String answerText) {
        return new TurnInput(
                question.questionId(),
                question.text(),
                question.groupId(),
                question.group(),
                answerText == null ? "" : answerText
        );
    }

    private String transcribe(AudioSnapshot snapshot, RuntimeContext context) {
        byte[] wav = toWav(snapshot.pcm16(), snapshot.sampleRate());
        SttCommand command = new SttCommand();
        command.setApiKey(context.apiKey);
        command.setModel(context.settings.sttModel());
        command.setFileBytes(wav);
        command.setTranslate(false);
        command.setSessionId(context.sessionId);
        return sttUseCase.transcribe(command);
    }

    private byte[] toWav(byte[] pcm16, int sampleRate) {
        int safeRate = sampleRate > 0 ? sampleRate : DEFAULT_SAMPLE_RATE;
        int dataSize = pcm16.length;
        int totalSize = 44 + dataSize;
        ByteBuffer buffer = ByteBuffer.allocate(totalSize).order(ByteOrder.LITTLE_ENDIAN);
        buffer.put("RIFF".getBytes());
        buffer.putInt(36 + dataSize);
        buffer.put("WAVE".getBytes());
        buffer.put("fmt ".getBytes());
        buffer.putInt(16);
        buffer.putShort((short) 1);
        buffer.putShort((short) 1);
        buffer.putInt(safeRate);
        buffer.putInt(safeRate * 2);
        buffer.putShort((short) 2);
        buffer.putShort((short) 16);
        buffer.put("data".getBytes());
        buffer.putInt(dataSize);
        buffer.put(pcm16);
        return buffer.array();
    }

    private long audioDurationMs(int pcm16Size, int sampleRate) {
        if (pcm16Size <= 0 || sampleRate <= 0) {
            return 0L;
        }
        double sampleCount = pcm16Size / 2.0;
        return Math.round((sampleCount / sampleRate) * 1000.0);
    }

    private boolean isContinuousGroupCompleted(TurnInput turnInput, NextQuestion nextQuestion, boolean exhausted) {
        if (turnInput == null || isBlank(turnInput.groupId())) {
            return false;
        }
        if (exhausted || nextQuestion == null || nextQuestion.isSkipped()) {
            return true;
        }
        String nextGroupId = nextQuestion.getGroupId();
        if (isBlank(nextGroupId)) {
            return false;
        }
        return !turnInput.groupId().equals(nextGroupId);
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

    private Map<String, Object> feedbackPolicyPayload(ModeType mode,
                                                      int groupBatchSize,
                                                      TurnBatchingPolicy.BatchReason reason) {
        Map<String, Object> policy = new LinkedHashMap<>();
        policy.put("mode", mode == null ? ModeType.IMMEDIATE.name() : mode.name());
        policy.put("groupBatchSize", Math.max(1, groupBatchSize));
        policy.put("reason", reason == null ? "" : reason.name());
        return policy;
    }

    private Map<String, Object> feedbackBatchPayload(List<FeedbackItem> items, boolean residual) {
        List<FeedbackItem> safeItems = items == null ? List.of() : items;
        Map<String, Object> batch = new LinkedHashMap<>();
        batch.put("size", safeItems.size());
        batch.put("isResidual", residual);
        batch.put("items", safeItems.stream().map(this::feedbackItemPayload).toList());
        return batch;
    }

    private Map<String, Object> feedbackItemPayload(FeedbackItem item) {
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("questionId", item.questionId());
        payload.put("questionText", item.questionText());
        payload.put("questionGroup", item.questionGroup() == null ? null : item.questionGroup().value());
        payload.put("answerText", item.answerText());
        payload.put("summary", item.feedback().getSummary());
        payload.put("correctionPoints", item.feedback().getCorrectionPoints());
        payload.put("recommendation", item.feedback().getRecommendation());
        payload.put("exampleAnswer", item.feedback().getExampleAnswer());
        payload.put("rulebookEvidence", item.feedback().getRulebookEvidence());
        return payload;
    }

    private Map<String, Object> nextActionPayload(SessionFlowPolicy.SessionAction action) {
        SessionFlowPolicy.SessionAction safeAction = action == null
                ? SessionFlowPolicy.SessionAction.askNext()
                : action;
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("type", safeAction.type() == SessionFlowPolicy.NextActionType.AUTO_STOP ? "auto_stop" : "ask_next");
        if (safeAction.reason() != null) {
            payload.put("reason", safeAction.reason().code());
        }
        return payload;
    }

    private Map<String, Object> questionPayload(String sessionId, long turnId, ModeType mode, NextQuestion question) {
        boolean exhausted = isQuestionExhausted(question);
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("sessionId", sessionId);
        payload.put("turnId", turnId);
        payload.put("question", questionNode(question, exhausted));
        payload.put("selection", selectionPayload(mode, exhausted));
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
        node.put("groupId", question.getGroupId());
        node.put("questionType", question.getQuestionType());
        return node;
    }

    private Map<String, Object> selectionPayload(ModeType mode, boolean exhausted) {
        Map<String, Object> selection = new LinkedHashMap<>();
        selection.put("mode", mode == null ? ModeType.IMMEDIATE.name() : mode.name());
        selection.put("exhausted", exhausted);
        if (exhausted) {
            selection.put("reason", SessionStopReason.QUESTION_EXHAUSTED.code());
        }
        return selection;
    }

    private boolean isQuestionExhausted(NextQuestion question) {
        return question == null || question.isSkipped();
    }

    private void emitSpeech(RuntimeContext context, long turnId, String role, String text) {
        if (context.isInactive() || isBlank(text)) {
            return;
        }
        Instant start = Instant.now();
        String phase = isBlank(role) ? "question" : role;
        try {
            byte[] wav = ttsGateway.synthesize(
                    context.apiKey,
                    context.settings.ttsModel(),
                    context.settings.ttsVoice(),
                    text
            );
            if (wav == null || wav.length == 0) {
                return;
            }
            metrics.recordTtsLatency(Duration.between(start, Instant.now()));
            context.emit("tts.audio", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "role", phase,
                    "phase", phase,
                    "sequence", context.nextTtsSequence(),
                    "text", text,
                    "audio", Base64.getEncoder().encodeToString(wav),
                    "mimeType", "audio/wav"
            ));
        } catch (Exception e) {
            metrics.incrementTtsError();
            context.emit("tts.error", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "message", defaultMessage(e.getMessage())
            ));
        }
    }

    private String toTtsText(List<FeedbackItem> items) {
        if (items == null || items.isEmpty()) {
            return "No feedback available.";
        }
        if (items.size() == 1) {
            return toTtsText(items.get(0).feedback());
        }
        StringBuilder builder = new StringBuilder();
        for (int i = 0; i < items.size(); i += 1) {
            FeedbackItem item = items.get(i);
            builder.append("Feedback ").append(i + 1).append('\n');
            if (!isBlank(item.questionText())) {
                builder.append(item.questionText()).append('\n');
            }
            builder.append(toTtsText(item.feedback())).append('\n');
        }
        return builder.toString().trim();
    }

    private String toTtsText(Feedback feedback) {
        StringBuilder builder = new StringBuilder();
        if (!isBlank(feedback.getSummary())) {
            builder.append(feedback.getSummary()).append('\n');
        }
        if (!feedback.getCorrectionPoints().isEmpty()) {
            builder.append(String.join("\n", feedback.getCorrectionPoints())).append('\n');
        }
        if (!feedback.getRecommendation().isEmpty()) {
            builder.append(String.join("\n", feedback.getRecommendation())).append('\n');
        }
        if (!isBlank(feedback.getExampleAnswer())) {
            builder.append(feedback.getExampleAnswer());
        }
        String text = builder.toString().trim();
        return text.isEmpty() ? "No feedback available." : text;
    }

    private void stopInternal(RuntimeContext context, boolean forced, String reason) {
        if (!context.close()) {
            return;
        }
        context.emit("session.stopped", Map.of(
                "sessionId", context.sessionId,
                "voiceSessionId", context.voiceSessionId,
                "forced", forced,
                "reason", normalizeStopReason(reason, forced ? "user_stop" : SessionStopReason.QUESTION_EXHAUSTED.code())
        ));
        contextByVoiceSessionId.remove(context.voiceSessionId, context);
    }

    private String normalizeStopReason(String reason, String fallback) {
        return isBlank(reason) ? fallback : reason;
    }

    private void completeTurnProcessing(RuntimeContext context) {
        context.finishTurnProcessing();
        if (context.hasBufferedAudio()) {
            flushTurn(context);
        }
    }

    private String defaultMessage(String message) {
        return isBlank(message) ? "Unknown voice session error" : message;
    }

    private Map<String, Object> errorPayload(String sessionId, long turnId, String message) {
        return Map.of(
                "sessionId", sessionId,
                "turnId", turnId,
                "message", message == null ? "Unknown error" : message
        );
    }

    private boolean isBlank(String value) {
        return value == null || value.isBlank();
    }

    private record RuntimeSettings(String sttModel,
                                   String feedbackModel,
                                   String feedbackLanguage,
                                   String ttsModel,
                                   String ttsVoice,
                                   int silenceDurationMs) {
        private static RuntimeSettings resolve(VoiceSessionOpenCommand command, VoicePolicy policy) {
            return new RuntimeSettings(
                    firstNonBlank(command.getSttModel(), policy.voiceSttModel()),
                    firstNonBlank(command.getFeedbackModel(), policy.voiceFeedbackModel()),
                    firstNonBlank(command.getFeedbackLanguage(), policy.voiceFeedbackLanguage()),
                    firstNonBlank(command.getTtsModel(), policy.voiceTtsModel()),
                    firstNonBlank(command.getTtsVoice(), policy.voiceTtsVoice()),
                    Math.max(0, policy.voiceSilenceDurationMs())
            );
        }
    }

    private record FeedbackPlan(List<TurnInput> inputs, TurnBatchingPolicy.BatchReason reason) {
    }

    private record FeedbackItem(String questionId,
                                String questionText,
                                QuestionGroup questionGroup,
                                String answerText,
                                Feedback feedback) {
    }

    private record TurnInput(String questionId,
                             String questionText,
                             String groupId,
                             String questionGroupText,
                             String answerText) {
        private QuestionGroup questionGroup() {
            return QuestionGroup.fromNullable(questionGroupText);
        }
    }

    private record QuestionSnapshot(String questionId,
                                    String text,
                                    String groupId,
                                    String group,
                                    String questionType) {
    }

    private record AudioSnapshot(byte[] pcm16, int sampleRate, long turnId, QuestionSnapshot question) {
    }

    private static final class RuntimeContext {
        private final String voiceSessionId;
        private final String sessionId;
        private final String apiKey;
        private final RuntimeSettings settings;
        private final Set<VoiceEventSink> sinks = ConcurrentHashMap.newKeySet();
        private final Deque<TurnInput> continuousTurns = new ConcurrentLinkedDeque<>();
        private final AtomicBoolean initialized = new AtomicBoolean(false);
        private final AtomicBoolean closed = new AtomicBoolean(false);
        private final AtomicBoolean turnProcessing = new AtomicBoolean(false);
        private final AtomicLong turnIdSequence = new AtomicLong(0L);
        private final AtomicLong ttsSequence = new AtomicLong(0L);
        private final Object audioLock = new Object();
        private final ByteArrayOutputStream pcmBuffer = new ByteArrayOutputStream();
        private int sampleRate = DEFAULT_SAMPLE_RATE;
        private NextQuestion currentQuestion;
        private long currentTurnId;

        private RuntimeContext(String voiceSessionId,
                               String sessionId,
                               String apiKey,
                               RuntimeSettings settings) {
            this.voiceSessionId = voiceSessionId;
            this.sessionId = sessionId;
            this.apiKey = apiKey;
            this.settings = settings;
        }

        private void addSink(VoiceEventSink sink) {
            if (sink != null) {
                sinks.add(sink);
            }
        }

        private void removeSink(VoiceEventSink sink) {
            if (sink != null) {
                sinks.remove(sink);
            }
        }

        private void emit(String event, Object payload) {
            for (VoiceEventSink sink : sinks) {
                if (!sink.send(event, payload)) {
                    sinks.remove(sink);
                }
            }
        }

        private boolean markInitialized() {
            return initialized.compareAndSet(false, true);
        }

        private boolean close() {
            return closed.compareAndSet(false, true);
        }

        private boolean isInactive() {
            return closed.get();
        }

        private boolean startTurnProcessing() {
            return turnProcessing.compareAndSet(false, true);
        }

        private void finishTurnProcessing() {
            turnProcessing.set(false);
        }

        private void appendAudio(byte[] pcm16, int sampleRate) {
            synchronized (audioLock) {
                if (pcm16.length > 0) {
                    pcmBuffer.write(pcm16, 0, pcm16.length);
                }
                if (sampleRate > 0) {
                    this.sampleRate = sampleRate;
                }
            }
        }

        private AudioSnapshot consumeAudioSnapshot() {
            synchronized (audioLock) {
                byte[] pcm16 = pcmBuffer.toByteArray();
                pcmBuffer.reset();
                int resolvedRate = sampleRate > 0 ? sampleRate : DEFAULT_SAMPLE_RATE;
                QuestionSnapshot question = snapshotQuestion();
                return new AudioSnapshot(pcm16, resolvedRate, currentTurnId, question);
            }
        }

        private boolean hasBufferedAudio() {
            synchronized (audioLock) {
                return pcmBuffer.size() > 0;
            }
        }

        private long nextTurnId() {
            return turnIdSequence.incrementAndGet();
        }

        private long nextTtsSequence() {
            return ttsSequence.incrementAndGet();
        }

        private void setCurrentQuestion(NextQuestion question, long turnId) {
            synchronized (audioLock) {
                this.currentQuestion = question;
                this.currentTurnId = turnId;
            }
        }

        private void clearCurrentQuestion() {
            synchronized (audioLock) {
                this.currentQuestion = null;
            }
        }

        private long currentTurnId() {
            return currentTurnId;
        }

        private QuestionSnapshot snapshotQuestion() {
            if (currentQuestion == null || currentQuestion.isSkipped()) {
                return null;
            }
            return new QuestionSnapshot(
                    currentQuestion.getQuestionId(),
                    currentQuestion.getText(),
                    currentQuestion.getGroupId(),
                    currentQuestion.getGroup(),
                    currentQuestion.getQuestionType()
            );
        }

        private void appendContinuousTurn(TurnInput turn) {
            continuousTurns.add(turn);
        }

        private List<TurnInput> pollAllContinuousTurns() {
            List<TurnInput> turns = new ArrayList<>();
            while (true) {
                TurnInput item = continuousTurns.pollFirst();
                if (item == null) {
                    return turns;
                }
                turns.add(item);
            }
        }
    }

    private static String firstNonBlank(String value, String fallback) {
        if (value == null || value.isBlank()) {
            return fallback;
        }
        return value;
    }
}
