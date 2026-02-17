package me.go_gradually.omypic.application.voice.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.policy.FeedbackModelPolicy;
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
import me.go_gradually.omypic.domain.feedback.CorrectionDetail;
import me.go_gradually.omypic.domain.feedback.Corrections;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.feedback.RecommendationDetail;
import me.go_gradually.omypic.domain.feedback.Recommendations;
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
import java.util.Comparator;
import java.util.Deque;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedDeque;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.LongSupplier;
import java.util.logging.Logger;

public class VoiceSessionUseCase {
    private static final Logger log = Logger.getLogger(VoiceSessionUseCase.class.getName());
    private static final String FEEDBACK_PROVIDER = "openai";
    private static final int DEFAULT_SAMPLE_RATE = 16000;
    private static final int MAX_RULEBOOK_DOCUMENTS_PER_TURN = 2;
    private static final int DEFAULT_EVENT_REPLAY_BUFFER_LIMIT = 256;
    private static final long DEFAULT_RECOVERY_RETENTION_MS = 600000L;
    private static final int DEFAULT_STOPPED_CONTEXT_MAX = 1000;
    private final SttUseCase sttUseCase;
    private final FeedbackUseCase feedbackUseCase;
    private final SessionUseCase sessionUseCase;
    private final QuestionUseCase questionUseCase;
    private final TtsGateway ttsGateway;
    private final AsyncExecutor asyncExecutor;
    private final VoicePolicy voicePolicy;
    private final MetricsPort metrics;
    private final int eventReplayBufferLimit;
    private final LongSupplier nowMillisSupplier;
    private final Map<String, RuntimeContext> contextByVoiceSessionId = new ConcurrentHashMap<>();

    public VoiceSessionUseCase(SttUseCase sttUseCase,
                               FeedbackUseCase feedbackUseCase,
                               SessionUseCase sessionUseCase,
                               QuestionUseCase questionUseCase,
                               TtsGateway ttsGateway,
                               AsyncExecutor asyncExecutor,
                               VoicePolicy voicePolicy,
                               MetricsPort metrics) {
        this(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics,
                DEFAULT_EVENT_REPLAY_BUFFER_LIMIT,
                System::currentTimeMillis
        );
    }

    VoiceSessionUseCase(SttUseCase sttUseCase,
                        FeedbackUseCase feedbackUseCase,
                        SessionUseCase sessionUseCase,
                        QuestionUseCase questionUseCase,
                        TtsGateway ttsGateway,
                        AsyncExecutor asyncExecutor,
                        VoicePolicy voicePolicy,
                        MetricsPort metrics,
                        int eventReplayBufferLimit) {
        this(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics,
                eventReplayBufferLimit,
                System::currentTimeMillis
        );
    }

    VoiceSessionUseCase(SttUseCase sttUseCase,
                        FeedbackUseCase feedbackUseCase,
                        SessionUseCase sessionUseCase,
                        QuestionUseCase questionUseCase,
                        TtsGateway ttsGateway,
                        AsyncExecutor asyncExecutor,
                        VoicePolicy voicePolicy,
                        MetricsPort metrics,
                        int eventReplayBufferLimit,
                        LongSupplier nowMillisSupplier) {
        this.sttUseCase = sttUseCase;
        this.feedbackUseCase = feedbackUseCase;
        this.sessionUseCase = sessionUseCase;
        this.questionUseCase = questionUseCase;
        this.ttsGateway = ttsGateway;
        this.asyncExecutor = asyncExecutor;
        this.voicePolicy = voicePolicy;
        this.metrics = metrics;
        this.eventReplayBufferLimit = Math.max(1, eventReplayBufferLimit);
        this.nowMillisSupplier = nowMillisSupplier == null ? System::currentTimeMillis : nowMillisSupplier;
    }

    public String open(VoiceSessionOpenCommand command) {
        purgeStoppedContexts();
        validateOpenCommand(command);
        sessionUseCase.getOrCreate(command.getSessionId());
        RuntimeContext context = createRuntimeContext(command);
        contextByVoiceSessionId.put(context.voiceSessionId, context);
        return context.voiceSessionId;
    }

    private void validateOpenCommand(VoiceSessionOpenCommand command) {
        if (command == null || isBlank(command.getSessionId()) || isBlank(command.getApiKey())) {
            throw new IllegalArgumentException("sessionId and apiKey are required");
        }
        FeedbackModelPolicy.validateOrThrow(command.getFeedbackModel());
    }

    private RuntimeContext createRuntimeContext(VoiceSessionOpenCommand command) {
        return new RuntimeContext(
                UUID.randomUUID().toString(),
                command.getSessionId(),
                command.getApiKey(),
                RuntimeSettings.resolve(command, voicePolicy),
                eventReplayBufferLimit
        );
    }

    public void registerSink(String voiceSessionId, VoiceEventSink sink) {
        registerSink(voiceSessionId, sink, null);
    }

    public void registerSink(String voiceSessionId, VoiceEventSink sink, Long sinceEventId) {
        purgeStoppedContexts();
        RuntimeContext context = requireContext(voiceSessionId);
        context.addSink(sink, sinceEventId);
        if (!context.markInitialized()) {
            return;
        }
        asyncExecutor.execute(() -> initializeSession(context));
    }

    public void unregisterSink(String voiceSessionId, VoiceEventSink sink) {
        purgeStoppedContexts();
        RuntimeContext context = contextByVoiceSessionId.get(voiceSessionId);
        if (context == null) {
            return;
        }
        context.removeSink(sink);
    }

    public RecoverySnapshot recover(String voiceSessionId, Long lastSeenEventId) {
        purgeStoppedContexts();
        RuntimeContext context = requireContext(voiceSessionId);
        long safeLastSeen = lastSeenEventId == null ? 0L : Math.max(0L, lastSeenEventId);
        return context.snapshotForRecovery(safeLastSeen);
    }

    public void appendAudio(VoiceAudioChunkCommand command) {
        purgeStoppedContexts();
        validateAudioCommand(command);
        RuntimeContext context = requireContext(command.getVoiceSessionId());
        if (context.isInactive()) {
            return;
        }
        if (!context.acceptChunkSequence(command.getSequence())) {
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
        purgeStoppedContexts();
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
        try {
            bootstrapFeedbackContext(context);
            emitSessionReady(context);
            sendQuestionPrompt(context);
        } catch (Exception e) {
            context.emit("error", errorPayload(context.sessionId, 0L, defaultMessage(e.getMessage())));
            stopInternal(context, false, "initialization_failed");
        }
    }

    private void emitSessionReady(RuntimeContext context) {
        context.emit("session.ready", Map.of(
                "sessionId", context.sessionId,
                "voiceSessionId", context.voiceSessionId
        ));
    }

    private void bootstrapFeedbackContext(RuntimeContext context) throws Exception {
        FeedbackCommand command = feedbackCommand(context, "");
        feedbackUseCase.bootstrapConversation(
                context.apiKey,
                command,
                context.settings.feedbackLanguage()
        );
    }

    private void sendQuestionPrompt(RuntimeContext context) {
        SessionState sessionState = sessionUseCase.getOrCreate(context.sessionId);
        NextQuestion nextQuestion = questionUseCase.nextQuestion(context.sessionId);
        long turnId = emitInitialQuestionPrompt(context, sessionState.getMode(), nextQuestion);
        continueOrStopAfterQuestionPrompt(context, nextQuestion, turnId);
    }

    private long emitInitialQuestionPrompt(RuntimeContext context, ModeType mode, NextQuestion question) {
        boolean exhausted = isQuestionExhausted(question);
        long turnId = exhausted ? 0L : context.nextTurnId();
        context.emit("question.prompt", questionPayload(context.sessionId, turnId, mode, question));
        return turnId;
    }

    private void continueOrStopAfterQuestionPrompt(RuntimeContext context, NextQuestion question, long turnId) {
        if (isQuestionExhausted(question)) {
            stopInternal(context, false, SessionStopReason.QUESTION_EXHAUSTED.code());
            return;
        }
        context.setCurrentQuestion(question, turnId);
        scheduleTurnPrefetch(context, question);
        emitSpeech(context, turnId, "question", question.getText());
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
            processTurnPipeline(context, snapshot, turnId, startedAt);
        } catch (Exception e) {
            context.emit("error", errorPayload(context.sessionId, turnId, defaultMessage(e.getMessage())));
        } finally {
            completeTurnProcessing(context);
        }
    }

    private void processTurnPipeline(RuntimeContext context, AudioSnapshot snapshot, long turnId, Instant startedAt) {
        emitTurnFlush(context, snapshot, turnId);
        String transcript = transcribe(snapshot, context);
        if (emitEmptyTranscriptIfNeeded(context, turnId, transcript)) {
            return;
        }
        emitFinalTranscript(context, turnId, transcript);
        processTurnFlow(context, snapshot.question(), transcript, turnId);
        metrics.recordVoiceTurnLatency(Duration.between(startedAt, Instant.now()));
    }

    private void emitTurnFlush(RuntimeContext context, AudioSnapshot snapshot, long turnId) {
        context.emit("turn.flush", Map.of(
                "sessionId", context.sessionId,
                "turnId", turnId,
                "bytes", snapshot.pcm16().length,
                "sampleRate", snapshot.sampleRate(),
                "durationMs", audioDurationMs(snapshot.pcm16().length, snapshot.sampleRate())
        ));
    }

    private boolean emitEmptyTranscriptIfNeeded(RuntimeContext context, long turnId, String transcript) {
        if (!isBlank(transcript)) {
            return false;
        }
        context.emit("stt.skipped", Map.of(
                "sessionId", context.sessionId,
                "turnId", turnId,
                "reason", "empty_transcript"
        ));
        context.emit("error", errorPayload(context.sessionId, turnId, "음성이 인식되지 않았습니다."));
        return true;
    }

    private void emitFinalTranscript(RuntimeContext context, long turnId, String transcript) {
        context.emit("stt.final", Map.of(
                "sessionId", context.sessionId,
                "turnId", turnId,
                "text", transcript
        ));
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
            boolean emittedFeedback = false;
            try {
                emittedFeedback = emitMainFeedback(context, turnId, mode, feedbackPlan, nextAction);
                emitResidualFeedback(context, turnId, mode, sessionState, nextAction, exhausted, emittedFeedback);
            } catch (Exception e) {
                emitFeedbackErrorAndContinue(context, turnId, e);
            }
            long nextQuestionTurnId = emitQuestionPromptAndTrack(context, mode, nextQuestion, exhausted, turnId);
            if (nextQuestionTurnId > 0) {
                emitSpeech(context, nextQuestionTurnId, "question", nextQuestion.getText());
            }
        } else {
            long nextQuestionTurnId = emitQuestionPromptAndTrack(context, mode, nextQuestion, exhausted, turnId);
            if (nextQuestionTurnId > 0) {
                emitSpeech(context, nextQuestionTurnId, "question", nextQuestion.getText());
            }
            boolean emittedFeedback = false;
            try {
                emittedFeedback = emitMainFeedback(context, turnId, mode, feedbackPlan, nextAction);
                emitResidualFeedback(context, turnId, mode, sessionState, nextAction, exhausted, emittedFeedback);
            } catch (Exception e) {
                emitFeedbackErrorAndContinue(context, turnId, e);
            }
        }

        if (nextAction.type() == SessionFlowPolicy.NextActionType.AUTO_STOP) {
            stopInternal(context, false, nextAction.reason().code());
        }
    }

    private void emitFeedbackErrorAndContinue(RuntimeContext context, long turnId, Exception error) {
        String failure = defaultMessage(error == null ? null : error.getMessage());
        context.emit("error", errorPayload(
                context.sessionId,
                turnId,
                "피드백 생성에 실패했습니다. 다음 질문으로 진행합니다. (" + failure + ")"
        ));
        log.warning(() -> "feedback generation failed but session continued sessionId="
                + context.sessionId
                + " turnId="
                + turnId
                + " reason="
                + failure);
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
        scheduleTurnPrefetch(context, nextQuestion);
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
        return inputs.stream()
                .map((input) -> generateFeedbackItem(context, input))
                .toList();
    }

    private FeedbackItem generateFeedbackItem(RuntimeContext context, TurnInput input) {
        return new FeedbackItem(
                input.questionId(),
                input.questionText(),
                input.questionGroup(),
                input.answerText(),
                generateFeedback(context, input)
        );
    }

    private Feedback generateFeedback(RuntimeContext context, TurnInput input) {
        FeedbackCommand command = feedbackCommand(context, input.answerText());
        FeedbackUseCase.PrefetchedTurnPrompt prefetch = context.consumePrefetchedTurnPrompt(input.questionId());
        if (prefetch != null) {
            return feedbackUseCase.generateFeedbackForTurnWithPrefetch(
                    context.apiKey,
                    command,
                    input.answerText(),
                    prefetch
            );
        }
        return feedbackUseCase.generateFeedbackForTurn(
                context.apiKey,
                command,
                input.questionText(),
                input.questionGroup(),
                input.answerText(),
                MAX_RULEBOOK_DOCUMENTS_PER_TURN
        );
    }

    private void scheduleTurnPrefetch(RuntimeContext context, NextQuestion question) {
        if (context.isInactive() || question == null || question.isSkipped() || isBlank(question.getQuestionId())) {
            return;
        }
        PrefetchTarget target = new PrefetchTarget(
                question.getQuestionId(),
                question.getText(),
                QuestionGroup.fromNullable(question.getGroup()),
                context.settings.feedbackLanguage()
        );
        asyncExecutor.execute(() -> prefetchTurnPrompt(context, target));
    }

    private void prefetchTurnPrompt(RuntimeContext context, PrefetchTarget target) {
        if (context.isInactive() || target == null || isBlank(target.questionId())) {
            return;
        }
        try {
            FeedbackUseCase.PrefetchedTurnPrompt prefetch = feedbackUseCase.prefetchTurnPrompt(
                    target.questionId(),
                    target.questionText(),
                    target.questionGroup(),
                    target.feedbackLanguage(),
                    MAX_RULEBOOK_DOCUMENTS_PER_TURN
            );
            context.storePrefetchedTurnPrompt(prefetch);
        } catch (Exception e) {
            log.fine(() -> "feedback prefetch skipped sessionId="
                    + context.sessionId
                    + " questionId="
                    + target.questionId()
                    + " reason="
                    + defaultMessage(e.getMessage()));
        }
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
        ByteBuffer buffer = wavBuffer(dataSize);
        writeWavHeader(buffer, safeRate, dataSize);
        buffer.put(pcm16);
        return buffer.array();
    }

    private ByteBuffer wavBuffer(int dataSize) {
        return ByteBuffer.allocate(44 + dataSize).order(ByteOrder.LITTLE_ENDIAN);
    }

    private void writeWavHeader(ByteBuffer buffer, int sampleRate, int dataSize) {
        writeRiffHeader(buffer, dataSize);
        writeFmtChunk(buffer, sampleRate);
        writeDataChunk(buffer, dataSize);
    }

    private void writeRiffHeader(ByteBuffer buffer, int dataSize) {
        buffer.put("RIFF".getBytes());
        buffer.putInt(36 + dataSize);
        buffer.put("WAVE".getBytes());
    }

    private void writeFmtChunk(ByteBuffer buffer, int sampleRate) {
        buffer.put("fmt ".getBytes());
        buffer.putInt(16);
        buffer.putShort((short) 1);
        buffer.putShort((short) 1);
        buffer.putInt(sampleRate);
        buffer.putInt(sampleRate * 2);
        buffer.putShort((short) 2);
        buffer.putShort((short) 16);
    }

    private void writeDataChunk(ByteBuffer buffer, int dataSize) {
        buffer.put("data".getBytes());
        buffer.putInt(dataSize);
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
        appendFeedbackItemMeta(payload, item);
        appendFeedbackItemContent(payload, item.feedback());
        return payload;
    }

    private void appendFeedbackItemMeta(Map<String, Object> payload, FeedbackItem item) {
        payload.put("questionId", item.questionId());
        payload.put("questionText", item.questionText());
        payload.put("questionGroup", item.questionGroup() == null ? null : item.questionGroup().value());
        payload.put("answerText", item.answerText());
    }

    private void appendFeedbackItemContent(Map<String, Object> payload, Feedback feedback) {
        payload.put("summary", feedback.getSummary());
        payload.put("corrections", correctionPayload(feedback.getCorrections()));
        payload.put("recommendations", recommendationPayload(feedback.getRecommendations()));
        payload.put("exampleAnswer", feedback.getExampleAnswer());
        payload.put("rulebookEvidence", feedback.getRulebookEvidence());
    }

    private Map<String, Object> correctionPayload(Corrections corrections) {
        Corrections safe = corrections == null ? new Corrections(null, null, null) : corrections;
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("grammar", correctionDetailPayload(safe.grammar()));
        payload.put("expression", correctionDetailPayload(safe.expression()));
        payload.put("logic", correctionDetailPayload(safe.logic()));
        return payload;
    }

    private Map<String, Object> correctionDetailPayload(CorrectionDetail detail) {
        CorrectionDetail safe = detail == null ? new CorrectionDetail("", "") : detail;
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("issue", safe.issue());
        payload.put("fix", safe.fix());
        return payload;
    }

    private Map<String, Object> recommendationPayload(Recommendations recommendations) {
        Recommendations safe = recommendations == null ? new Recommendations(null, null, null) : recommendations;
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("filler", recommendationDetailPayload(safe.filler()));
        payload.put("adjective", recommendationDetailPayload(safe.adjective()));
        payload.put("adverb", recommendationDetailPayload(safe.adverb()));
        return payload;
    }

    private Map<String, Object> recommendationDetailPayload(RecommendationDetail detail) {
        RecommendationDetail safe = detail == null ? new RecommendationDetail("", "") : detail;
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("term", safe.term());
        payload.put("usage", safe.usage());
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
        String phase = resolveSpeechPhase(role);
        try {
            emitSpeechAudio(context, turnId, phase, text);
        } catch (Exception e) {
            emitSpeechError(context, turnId, e);
        }
    }

    private String resolveSpeechPhase(String role) {
        return isBlank(role) ? "question" : role;
    }

    private void emitSpeechAudio(RuntimeContext context, long turnId, String phase, String text) throws Exception {
        Instant start = Instant.now();
        byte[] wav = synthesizeSpeech(context, text);
        if (wav == null || wav.length == 0) {
            return;
        }
        metrics.recordTtsLatency(Duration.between(start, Instant.now()));
        emitSpeechEvent(context, turnId, phase, text, wav);
    }

    private byte[] synthesizeSpeech(RuntimeContext context, String text) throws Exception {
        return ttsGateway.synthesize(
                context.apiKey,
                context.settings.ttsModel(),
                context.settings.ttsVoice(),
                text
        );
    }

    private void emitSpeechEvent(RuntimeContext context, long turnId, String phase, String text, byte[] wav) {
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
    }

    private void emitSpeechError(RuntimeContext context, long turnId, Exception e) {
        metrics.incrementTtsError();
        context.emit("tts.error", Map.of(
                "sessionId", context.sessionId,
                "turnId", turnId,
                "message", defaultMessage(e.getMessage())
        ));
    }

    private String toTtsText(List<FeedbackItem> items) {
        if (items == null || items.isEmpty()) {
            return "No feedback available.";
        }
        if (items.size() == 1) {
            return toTtsText(items.get(0).feedback());
        }
        return joinFeedbackSpeech(items);
    }

    private String joinFeedbackSpeech(List<FeedbackItem> items) {
        StringBuilder builder = new StringBuilder();
        for (int index = 0; index < items.size(); index += 1) {
            appendFeedbackSpeech(builder, items.get(index), index + 1);
        }
        return builder.toString().trim();
    }

    private void appendFeedbackSpeech(StringBuilder builder, FeedbackItem item, int order) {
        builder.append("Feedback ").append(order).append('\n');
        if (!isBlank(item.questionText())) {
            builder.append(item.questionText()).append('\n');
        }
        builder.append(toTtsText(item.feedback())).append('\n');
    }

    private String toTtsText(Feedback feedback) {
        String text = String.join("\n", ttsSections(feedback)).trim();
        return text.isEmpty() ? "No feedback available." : text;
    }

    private List<String> ttsSections(Feedback feedback) {
        List<String> sections = new ArrayList<>();
        appendIfPresent(sections, feedback.getSummary());
        appendIfPresent(sections, String.join("\n", feedback.getCorrectionPoints()));
        appendIfPresent(sections, String.join("\n", feedback.getRecommendation()));
        appendIfPresent(sections, feedback.getExampleAnswer());
        return sections;
    }

    private void appendIfPresent(List<String> sections, String text) {
        if (!isBlank(text)) {
            sections.add(text);
        }
    }

    private void stopInternal(RuntimeContext context, boolean forced, String reason) {
        if (!context.close()) {
            return;
        }
        String stopReason = normalizeStopReason(reason, forced ? "user_stop" : SessionStopReason.QUESTION_EXHAUSTED.code());
        context.markStopped(stopReason, nowMillisSupplier.getAsLong());
        context.emit("session.stopped", Map.of(
                "sessionId", context.sessionId,
                "voiceSessionId", context.voiceSessionId,
                "forced", forced,
                "reason", stopReason
        ));
    }

    private void purgeStoppedContexts() {
        long now = nowMillisSupplier.getAsLong();
        long retentionMs = voicePolicy.voiceRecoveryRetentionMs();
        if (retentionMs <= 0L) {
            retentionMs = DEFAULT_RECOVERY_RETENTION_MS;
        }
        int stoppedContextMax = voicePolicy.voiceStoppedContextMax();
        if (stoppedContextMax <= 0) {
            stoppedContextMax = DEFAULT_STOPPED_CONTEXT_MAX;
        }

        List<Map.Entry<String, RuntimeContext>> retainedStoppedEntries = new ArrayList<>();
        for (Map.Entry<String, RuntimeContext> entry : contextByVoiceSessionId.entrySet()) {
            RuntimeContext context = entry.getValue();
            if (context == null || !context.isInactive()) {
                continue;
            }
            if (context.isStoppedExpired(now, retentionMs)) {
                contextByVoiceSessionId.remove(entry.getKey(), context);
                continue;
            }
            retainedStoppedEntries.add(entry);
        }

        if (retainedStoppedEntries.isEmpty()) {
            return;
        }
        if (stoppedContextMax <= 0) {
            for (Map.Entry<String, RuntimeContext> entry : retainedStoppedEntries) {
                contextByVoiceSessionId.remove(entry.getKey(), entry.getValue());
            }
            return;
        }
        if (retainedStoppedEntries.size() <= stoppedContextMax) {
            return;
        }
        retainedStoppedEntries.sort(Comparator.comparingLong(entry -> entry.getValue().stoppedAtEpochMs()));
        int removeCount = retainedStoppedEntries.size() - stoppedContextMax;
        for (int i = 0; i < removeCount; i += 1) {
            Map.Entry<String, RuntimeContext> entry = retainedStoppedEntries.get(i);
            contextByVoiceSessionId.remove(entry.getKey(), entry.getValue());
        }
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

    private record PrefetchTarget(String questionId,
                                  String questionText,
                                  QuestionGroup questionGroup,
                                  String feedbackLanguage) {
    }

    private record VoiceEventRecord(long eventId, String event, Object payload) {
    }

    public record RecoveryQuestion(String id,
                                   String text,
                                   String group,
                                   String groupId,
                                   String questionType) {
    }

    public record RecoverySnapshot(String sessionId,
                                   String voiceSessionId,
                                   boolean active,
                                   boolean stopped,
                                   String stopReason,
                                   long currentTurnId,
                                   RecoveryQuestion currentQuestion,
                                   boolean turnProcessing,
                                   boolean hasBufferedAudio,
                                   Long lastAcceptedChunkSequence,
                                   long latestEventId,
                                   long replayFromEventId,
                                   boolean gapDetected) {
    }

    private static final class RuntimeContext {
        private static final int PREFETCH_CACHE_LIMIT = 2;
        private final String voiceSessionId;
        private final String sessionId;
        private final String apiKey;
        private final RuntimeSettings settings;
        private final int eventReplayBufferLimit;
        private final Set<VoiceEventSink> sinks = ConcurrentHashMap.newKeySet();
        private final Deque<TurnInput> continuousTurns = new ConcurrentLinkedDeque<>();
        private final Deque<VoiceEventRecord> replayEvents = new ConcurrentLinkedDeque<>();
        private final LinkedHashMap<String, FeedbackUseCase.PrefetchedTurnPrompt> prefetchByQuestionId = new LinkedHashMap<>();
        private final Object eventLock = new Object();
        private final AtomicBoolean initialized = new AtomicBoolean(false);
        private final AtomicBoolean closed = new AtomicBoolean(false);
        private final AtomicBoolean turnProcessing = new AtomicBoolean(false);
        private final AtomicLong turnIdSequence = new AtomicLong(0L);
        private final AtomicLong ttsSequence = new AtomicLong(0L);
        private final AtomicLong eventIdSequence = new AtomicLong(0L);
        private final Object audioLock = new Object();
        private final ByteArrayOutputStream pcmBuffer = new ByteArrayOutputStream();
        private int sampleRate = DEFAULT_SAMPLE_RATE;
        private volatile String stopReason = "";
        private volatile long stoppedAtEpochMs;
        private Long lastAcceptedChunkSequence;
        private NextQuestion currentQuestion;
        private long currentTurnId;

        private RuntimeContext(String voiceSessionId,
                               String sessionId,
                               String apiKey,
                               RuntimeSettings settings,
                               int eventReplayBufferLimit) {
            this.voiceSessionId = voiceSessionId;
            this.sessionId = sessionId;
            this.apiKey = apiKey;
            this.settings = settings;
            this.eventReplayBufferLimit = Math.max(1, eventReplayBufferLimit);
        }

        private void addSink(VoiceEventSink sink, Long sinceEventId) {
            if (sink != null) {
                synchronized (eventLock) {
                    if (sinceEventId != null && !replayEvents(sink, sinceEventId)) {
                        return;
                    }
                    sinks.add(sink);
                }
            }
        }

        private void removeSink(VoiceEventSink sink) {
            if (sink != null) {
                sinks.remove(sink);
            }
        }

        private void emit(String event, Object payload) {
            synchronized (eventLock) {
                VoiceEventRecord record = createEventRecord(event, payload);
                replayEvents.addLast(record);
                trimReplayEvents();
                for (VoiceEventSink sink : sinks) {
                    if (!sink.send(record.event(), record.payload())) {
                        sinks.remove(sink);
                    }
                }
            }
        }

        private VoiceEventRecord createEventRecord(String event, Object payload) {
            long eventId = eventIdSequence.incrementAndGet();
            return new VoiceEventRecord(eventId, event, appendEventId(payload, eventId));
        }

        private Object appendEventId(Object payload, long eventId) {
            if (payload instanceof Map<?, ?> raw) {
                Map<String, Object> enriched = new LinkedHashMap<>();
                for (Map.Entry<?, ?> entry : raw.entrySet()) {
                    enriched.put(String.valueOf(entry.getKey()), entry.getValue());
                }
                enriched.put("eventId", eventId);
                return enriched;
            }
            Map<String, Object> wrapped = new LinkedHashMap<>();
            wrapped.put("eventId", eventId);
            wrapped.put("data", payload);
            return wrapped;
        }

        private void trimReplayEvents() {
            while (replayEvents.size() > eventReplayBufferLimit) {
                replayEvents.pollFirst();
            }
        }

        private boolean replayEvents(VoiceEventSink sink, Long sinceEventId) {
            long since = sinceEventId == null ? 0L : Math.max(0L, sinceEventId);
            for (VoiceEventRecord record : replayEvents) {
                if (record.eventId() <= since) {
                    continue;
                }
                if (!sink.send(record.event(), record.payload())) {
                    return false;
                }
            }
            return true;
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

        private void markStopped(String stopReason, long stoppedAtEpochMs) {
            this.stopReason = stopReason == null ? "" : stopReason;
            this.stoppedAtEpochMs = stoppedAtEpochMs;
        }

        private boolean isStoppedExpired(long nowEpochMs, long retentionMs) {
            if (!isInactive()) {
                return false;
            }
            if (retentionMs < 0L) {
                return false;
            }
            long stoppedAt = stoppedAtEpochMs;
            if (stoppedAt <= 0L) {
                return false;
            }
            return nowEpochMs - stoppedAt >= retentionMs;
        }

        private long stoppedAtEpochMs() {
            return stoppedAtEpochMs <= 0L ? Long.MAX_VALUE : stoppedAtEpochMs;
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

        private boolean acceptChunkSequence(Long sequence) {
            if (sequence == null) {
                return true;
            }
            synchronized (audioLock) {
                if (lastAcceptedChunkSequence != null && sequence <= lastAcceptedChunkSequence) {
                    return false;
                }
                lastAcceptedChunkSequence = sequence;
                return true;
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
                this.prefetchByQuestionId.clear();
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

        private void storePrefetchedTurnPrompt(FeedbackUseCase.PrefetchedTurnPrompt prefetch) {
            if (prefetch == null || prefetch.questionId() == null || prefetch.questionId().isBlank()) {
                return;
            }
            synchronized (audioLock) {
                if (currentQuestion == null || currentQuestion.isSkipped()) {
                    return;
                }
                if (!prefetch.questionId().equals(currentQuestion.getQuestionId())) {
                    return;
                }
                prefetchByQuestionId.put(prefetch.questionId(), prefetch);
                trimPrefetchCache();
            }
        }

        private FeedbackUseCase.PrefetchedTurnPrompt consumePrefetchedTurnPrompt(String questionId) {
            if (questionId == null || questionId.isBlank()) {
                return null;
            }
            synchronized (audioLock) {
                return prefetchByQuestionId.remove(questionId);
            }
        }

        private void trimPrefetchCache() {
            if (prefetchByQuestionId.size() <= PREFETCH_CACHE_LIMIT) {
                return;
            }
            Iterator<String> keys = prefetchByQuestionId.keySet().iterator();
            while (prefetchByQuestionId.size() > PREFETCH_CACHE_LIMIT && keys.hasNext()) {
                keys.next();
                keys.remove();
            }
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

        private RecoverySnapshot snapshotForRecovery(long lastSeenEventId) {
            long safeLastSeenEventId = Math.max(0L, lastSeenEventId);
            QuestionSnapshot question;
            long turnId;
            boolean bufferedAudio;
            Long acceptedChunkSequence;
            synchronized (audioLock) {
                question = snapshotQuestion();
                turnId = currentTurnId;
                bufferedAudio = pcmBuffer.size() > 0;
                acceptedChunkSequence = lastAcceptedChunkSequence;
            }

            long oldestEventId;
            long latestEventId;
            synchronized (eventLock) {
                oldestEventId = replayEvents.isEmpty() ? 0L : replayEvents.peekFirst().eventId();
                latestEventId = eventIdSequence.get();
            }
            long replayFromEventId = safeLastSeenEventId;
            boolean gapDetected = false;
            if (oldestEventId > 0L) {
                long minimumRewindPoint = oldestEventId - 1L;
                replayFromEventId = Math.max(safeLastSeenEventId, minimumRewindPoint);
                gapDetected = safeLastSeenEventId < minimumRewindPoint;
            }

            return new RecoverySnapshot(
                    sessionId,
                    voiceSessionId,
                    !isInactive(),
                    isInactive(),
                    stopReason,
                    turnId,
                    toRecoveryQuestion(question),
                    turnProcessing.get(),
                    bufferedAudio,
                    acceptedChunkSequence,
                    latestEventId,
                    replayFromEventId,
                    gapDetected
            );
        }

        private RecoveryQuestion toRecoveryQuestion(QuestionSnapshot question) {
            if (question == null) {
                return null;
            }
            return new RecoveryQuestion(
                    question.questionId(),
                    question.text(),
                    question.group(),
                    question.groupId(),
                    question.questionType()
            );
        }
    }

    private static String firstNonBlank(String value, String fallback) {
        if (value == null || value.isBlank()) {
            return fallback;
        }
        return value;
    }
}
