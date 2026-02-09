package me.go_gradually.omypic.application.realtime.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.realtime.model.*;
import me.go_gradually.omypic.application.realtime.policy.RealtimePolicy;
import me.go_gradually.omypic.application.realtime.port.RealtimeAudioGateway;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.usecase.TtsUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;

import java.time.Duration;
import java.time.Instant;
import java.util.Base64;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

public class RealtimeVoiceUseCase {
    private final RealtimeAudioGateway realtimeAudioGateway;
    private final FeedbackUseCase feedbackUseCase;
    private final TtsUseCase ttsUseCase;
    private final SessionUseCase sessionUseCase;
    private final AsyncExecutor asyncExecutor;
    private final RealtimePolicy realtimePolicy;
    private final MetricsPort metrics;

    public RealtimeVoiceUseCase(RealtimeAudioGateway realtimeAudioGateway,
                                FeedbackUseCase feedbackUseCase,
                                TtsUseCase ttsUseCase,
                                SessionUseCase sessionUseCase,
                                AsyncExecutor asyncExecutor,
                                RealtimePolicy realtimePolicy,
                                MetricsPort metrics) {
        this.realtimeAudioGateway = realtimeAudioGateway;
        this.feedbackUseCase = feedbackUseCase;
        this.ttsUseCase = ttsUseCase;
        this.sessionUseCase = sessionUseCase;
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
                        if (context.closed.get() || isBlank(delta)) {
                            return;
                        }
                        context.send("stt.partial", Map.of("sessionId", context.sessionId, "text", delta));
                    }

                    @Override
                    public void onFinalTranscript(String text) {
                        if (context.closed.get() || isBlank(text)) {
                            return;
                        }
                        handleFinalTranscript(context, text);
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
        return context;
    }

    private void handleFinalTranscript(RuntimeContext context, String text) {
        long turnId = context.turnSequence.incrementAndGet();
        context.activeTurn.set(turnId);
        context.send("stt.final", Map.of("sessionId", context.sessionId, "turnId", turnId, "text", text));
        sessionUseCase.appendSegment(context.sessionId, text);
        Instant startedAt = Instant.now();
        asyncExecutor.execute(() -> processTurn(context, turnId, text, startedAt));
    }

    private void processTurn(RuntimeContext context, long turnId, String transcript, Instant startedAt) {
        RuntimeSettings settings = context.settings;
        try {
            FeedbackCommand feedbackCommand = new FeedbackCommand();
            feedbackCommand.setSessionId(context.sessionId);
            feedbackCommand.setText(transcript);
            feedbackCommand.setProvider(settings.feedbackProvider);
            feedbackCommand.setModel(settings.feedbackModel);
            feedbackCommand.setFeedbackLanguage(settings.feedbackLanguage);
            String feedbackApiKey = firstNonBlank(settings.feedbackApiKey, context.apiKey);

            FeedbackResult result = feedbackUseCase.generateFeedback(feedbackApiKey, feedbackCommand);
            if (!result.isGenerated()) {
                context.send("feedback.skipped", Map.of("sessionId", context.sessionId, "turnId", turnId));
                context.send("turn.completed", Map.of("sessionId", context.sessionId, "turnId", turnId, "cancelled", isCancelled(context, turnId)));
                metrics.recordRealtimeTurnLatency(Duration.between(startedAt, Instant.now()));
                return;
            }

            Feedback feedback = result.getFeedback();
            if (!isCancelled(context, turnId) && !context.closed.get()) {
                context.send("feedback.final", feedbackPayload(context.sessionId, turnId, feedback));
            }

            String ttsText = toTtsText(feedback);
            ttsUseCase.streamUntil(
                    context.apiKey,
                    new TtsCommand(ttsText, settings.ttsVoice),
                    bytes -> {
                        if (context.closed.get() || isCancelled(context, turnId)) {
                            return;
                        }
                        String base64 = Base64.getEncoder().encodeToString(bytes);
                        context.send("tts.chunk", Map.of("sessionId", context.sessionId, "turnId", turnId, "audio", base64));
                    },
                    () -> context.closed.get() || isCancelled(context, turnId)
            );

            context.send("turn.completed", Map.of("sessionId", context.sessionId, "turnId", turnId, "cancelled", isCancelled(context, turnId)));
            metrics.recordRealtimeTurnLatency(Duration.between(startedAt, Instant.now()));
        } catch (Exception e) {
            context.send("error", Map.of(
                    "sessionId", context.sessionId,
                    "turnId", turnId,
                    "message", defaultMessage(e.getMessage())
            ));
            metrics.recordRealtimeTurnLatency(Duration.between(startedAt, Instant.now()));
        }
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
        private final AtomicBoolean closed = new AtomicBoolean(false);
        private volatile RuntimeSettings settings;
        private volatile RealtimeAudioSession audioSession;

        private RuntimeContext(String sessionId, String apiKey, RealtimeVoiceEventSink sink, RuntimeSettings settings) {
            this.sessionId = sessionId;
            this.apiKey = apiKey;
            this.sink = sink;
            this.settings = settings;
        }

        @Override
        public void appendAudio(String base64Audio) {
            if (closed.get() || isBlank(base64Audio) || audioSession == null) {
                return;
            }
            // User speech takes priority over current playback.
            cancelResponse();
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
