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
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.ArrayList;
import java.util.Base64;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Predicate;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyInt;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.doThrow;
import static org.mockito.Mockito.lenient;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.when;

@ExtendWith(MockitoExtension.class)
class VoiceSessionUseCaseTest {

    @Mock
    private SttUseCase sttUseCase;
    @Mock
    private FeedbackUseCase feedbackUseCase;
    @Mock
    private SessionUseCase sessionUseCase;
    @Mock
    private QuestionUseCase questionUseCase;
    @Mock
    private TtsGateway ttsGateway;
    @Mock
    private AsyncExecutor asyncExecutor;
    @Mock
    private VoicePolicy voicePolicy;
    @Mock
    private MetricsPort metrics;

    private VoiceSessionUseCase useCase;

    @BeforeEach
    void setUp() throws Exception {
        useCase = new VoiceSessionUseCase(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics
        );

        when(voicePolicy.voiceSttModel()).thenReturn("gpt-4o-mini-transcribe");
        when(voicePolicy.voiceFeedbackModel()).thenReturn("gpt-4o-mini");
        when(voicePolicy.voiceFeedbackLanguage()).thenReturn("ko");
        when(voicePolicy.voiceTtsModel()).thenReturn("gpt-4o-mini-tts");
        when(voicePolicy.voiceTtsVoice()).thenReturn("alloy");
        when(voicePolicy.voiceSilenceDurationMs()).thenReturn(1200);
        when(voicePolicy.voiceRecoveryRetentionMs()).thenReturn(600_000L);
        when(voicePolicy.voiceStoppedContextMax()).thenReturn(1000);

        lenient().doAnswer(invocation -> {
            Runnable task = invocation.getArgument(0);
            task.run();
            return null;
        }).when(asyncExecutor).execute(any(Runnable.class));

        lenient().when(sttUseCase.transcribe(any(SttCommand.class))).thenReturn("answer text");
        lenient().when(feedbackUseCase.generateFeedbackForTurn(
                anyString(),
                any(FeedbackCommand.class),
                anyString(),
                any(),
                anyString(),
                anyInt()
        )).thenReturn(sampleFeedback());
        lenient().when(ttsGateway.synthesize(anyString(), anyString(), anyString(), anyString()))
                .thenReturn(new byte[]{1, 2, 3, 4});
    }

    @Test
    void immediateMode_emitsFeedbackSpeechBeforeNextQuestionSpeech() throws Exception {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby")
        );

        String voiceSessionId = useCase.open(openCommand("s1"));
        List<EventRecord> events = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(events));
        verify(feedbackUseCase, times(1))
                .bootstrapConversation(anyString(), any(FeedbackCommand.class), anyString());

        events.clear();
        useCase.appendAudio(audioChunk(voiceSessionId));

        int feedbackFinalIndex = firstIndex(events, event -> "feedback.final".equals(event.type()));
        int feedbackSpeechIndex = firstIndex(events, event -> isTtsRole(event, "feedback"));
        int questionPromptIndex = firstIndex(events, event -> "question.prompt".equals(event.type()));
        int questionSpeechIndex = firstIndex(events, event -> isTtsRole(event, "question"));

        assertTrue(feedbackFinalIndex >= 0);
        assertTrue(feedbackSpeechIndex >= 0);
        assertTrue(questionPromptIndex >= 0);
        assertTrue(questionSpeechIndex >= 0);
        assertTrue(feedbackFinalIndex < feedbackSpeechIndex);
        assertTrue(feedbackSpeechIndex < questionPromptIndex);
        assertTrue(questionPromptIndex < questionSpeechIndex);

        long feedbackSequence = ttsSequence(events.get(feedbackSpeechIndex).payload());
        long questionSequence = ttsSequence(events.get(questionSpeechIndex).payload());
        assertTrue(feedbackSequence < questionSequence);
    }

    @Test
    void continuousMode_keepsFeedbackAsTextOnlyAndSpeaksQuestionOnly() {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.CONTINUOUS, 1);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby")
        );

        String voiceSessionId = useCase.open(openCommand("s1"));
        List<EventRecord> events = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(events));

        events.clear();
        useCase.appendAudio(audioChunk(voiceSessionId));

        int questionPromptIndex = firstIndex(events, event -> "question.prompt".equals(event.type()));
        int questionSpeechIndex = firstIndex(events, event -> isTtsRole(event, "question"));
        int feedbackFinalIndex = firstIndex(events, event -> "feedback.final".equals(event.type()));
        int feedbackSpeechIndex = firstIndex(events, event -> isTtsRole(event, "feedback"));
        long questionSpeechCount = events.stream().filter(event -> isTtsRole(event, "question")).count();

        assertTrue(questionPromptIndex >= 0);
        assertTrue(questionSpeechIndex >= 0);
        assertTrue(feedbackFinalIndex >= 0);
        assertTrue(questionPromptIndex < questionSpeechIndex);
        assertTrue(questionSpeechIndex < feedbackFinalIndex);
        assertEquals(-1, feedbackSpeechIndex);
        assertEquals(1L, questionSpeechCount);
    }

    @Test
    void immediateMode_usesPrefetchedTurnPromptWhenAvailable() {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby")
        );
        when(feedbackUseCase.prefetchTurnPrompt(anyString(), anyString(), any(), anyString(), anyInt()))
                .thenAnswer(invocation -> new FeedbackUseCase.PrefetchedTurnPrompt(
                        invocation.getArgument(0),
                        invocation.getArgument(1),
                        List.of(),
                        "ko",
                        "prefetched-system-prompt"
                ));
        when(feedbackUseCase.generateFeedbackForTurnWithPrefetch(
                anyString(),
                any(FeedbackCommand.class),
                anyString(),
                any()
        )).thenReturn(sampleFeedback());

        String voiceSessionId = useCase.open(openCommand("s1"));
        List<EventRecord> events = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(events));

        events.clear();
        useCase.appendAudio(audioChunk(voiceSessionId));

        verify(feedbackUseCase).generateFeedbackForTurnWithPrefetch(
                anyString(),
                any(FeedbackCommand.class),
                anyString(),
                any()
        );
        verify(feedbackUseCase, never()).generateFeedbackForTurn(
                anyString(),
                any(FeedbackCommand.class),
                anyString(),
                any(),
                anyString(),
                anyInt()
        );
    }

    @Test
    void immediateMode_continuesToNextQuestionWhenFeedbackFails() {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby")
        );
        when(feedbackUseCase.generateFeedbackForTurn(
                anyString(),
                any(FeedbackCommand.class),
                anyString(),
                any(),
                anyString(),
                anyInt()
        )).thenThrow(new IllegalStateException("LLM feedback failed: timeout"));

        String voiceSessionId = useCase.open(openCommand("s1"));
        List<EventRecord> events = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(events));

        events.clear();
        useCase.appendAudio(audioChunk(voiceSessionId));

        int errorIndex = firstIndex(events, event -> "error".equals(event.type()));
        int feedbackFinalIndex = firstIndex(events, event -> "feedback.final".equals(event.type()));
        int questionPromptIndex = firstIndex(events, event -> "question.prompt".equals(event.type()));
        int questionSpeechIndex = firstIndex(events, event -> isTtsRole(event, "question"));

        assertTrue(errorIndex >= 0);
        assertEquals(-1, feedbackFinalIndex);
        assertTrue(questionPromptIndex >= 0);
        assertTrue(questionSpeechIndex >= 0);
        assertTrue(errorIndex < questionPromptIndex);
    }

    @Test
    void appendAudio_ignoresDuplicateOrOlderSequence() {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby")
        );

        String voiceSessionId = useCase.open(openCommand("s1"));
        useCase.registerSink(voiceSessionId, capture(new ArrayList<>()));

        useCase.appendAudio(audioChunk(voiceSessionId, 5L));
        useCase.appendAudio(audioChunk(voiceSessionId, 5L));
        useCase.appendAudio(audioChunk(voiceSessionId, 4L));

        verify(sttUseCase, times(1)).transcribe(any(SttCommand.class));
    }

    @Test
    void initializeSession_stopsWhenBootstrapFails() throws Exception {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        doThrow(new IllegalStateException("bootstrap failed"))
                .when(feedbackUseCase)
                .bootstrapConversation(anyString(), any(FeedbackCommand.class), anyString());

        String voiceSessionId = useCase.open(openCommand("s1"));
        List<EventRecord> events = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(events));

        int readyIndex = firstIndex(events, event -> "session.ready".equals(event.type()));
        int errorIndex = firstIndex(events, event -> "error".equals(event.type()));
        int stoppedIndex = firstIndex(events, event -> "session.stopped".equals(event.type()));

        assertEquals(-1, readyIndex);
        assertTrue(errorIndex >= 0);
        assertTrue(stoppedIndex > errorIndex);
    }

    @Test
    void emittedEvents_includeMonotonicEventId() {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby")
        );

        String voiceSessionId = useCase.open(openCommand("s1"));
        List<EventRecord> events = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(events));

        events.clear();
        useCase.appendAudio(audioChunk(voiceSessionId));

        long previous = 0L;
        for (EventRecord event : events) {
            long eventId = eventId(event.payload());
            assertTrue(eventId > previous);
            previous = eventId;
        }
    }

    @Test
    void registerSink_replaysEventsAfterSinceEventId() {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby")
        );

        String voiceSessionId = useCase.open(openCommand("s1"));
        List<EventRecord> events = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(events));
        useCase.appendAudio(audioChunk(voiceSessionId));

        long sinceEventId = eventId(events.get(0).payload());
        List<Long> expectedEventIds = events.stream()
                .map(EventRecord::payload)
                .map(this::eventId)
                .filter(id -> id > sinceEventId)
                .toList();

        List<EventRecord> replayed = new ArrayList<>();
        useCase.registerSink(voiceSessionId, capture(replayed), sinceEventId);
        List<Long> replayedEventIds = replayed.stream()
                .map(EventRecord::payload)
                .map(this::eventId)
                .toList();

        assertEquals(expectedEventIds, replayedEventIds);
    }

    @Test
    void recover_marksGapDetectedWhenReplayBufferTrimmed() {
        VoiceSessionUseCase smallReplayUseCase = new VoiceSessionUseCase(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics,
                4
        );

        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.IMMEDIATE, null);
        when(sessionUseCase.getOrCreate("s1")).thenReturn(state);
        when(questionUseCase.nextQuestion("s1")).thenReturn(
                question("q-1", "question-1", "g-1", "travel"),
                question("q-2", "question-2", "g-2", "hobby"),
                question("q-3", "question-3", "g-3", "hobby"),
                question("q-4", "question-4", "g-4", "hobby"),
                question("q-5", "question-5", "g-5", "hobby"),
                question("q-6", "question-6", "g-6", "hobby"),
                question("q-7", "question-7", "g-7", "hobby")
        );

        String voiceSessionId = smallReplayUseCase.open(openCommand("s1"));
        smallReplayUseCase.registerSink(voiceSessionId, capture(new ArrayList<>()));
        for (long sequence = 1L; sequence <= 6L; sequence += 1L) {
            smallReplayUseCase.appendAudio(audioChunk(voiceSessionId, sequence));
        }

        VoiceSessionUseCase.RecoverySnapshot snapshot = smallReplayUseCase.recover(voiceSessionId, 0L);
        assertTrue(snapshot.gapDetected());
        assertEquals(snapshot.latestEventId() - 4L, snapshot.replayFromEventId());
        assertNotNull(snapshot.currentQuestion());
    }

    @Test
    void purgeStoppedContexts_removesExpiredSessionAfterTtl() {
        AtomicLong now = new AtomicLong(1_000L);
        VoiceSessionUseCase ttlUseCase = new VoiceSessionUseCase(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics,
                256,
                now::get
        );
        when(voicePolicy.voiceRecoveryRetentionMs()).thenReturn(600_000L);
        when(voicePolicy.voiceStoppedContextMax()).thenReturn(1000);
        when(sessionUseCase.getOrCreate("s-ttl")).thenReturn(new SessionState(SessionId.of("s-ttl")));
        when(sessionUseCase.getOrCreate("s-trigger")).thenReturn(new SessionState(SessionId.of("s-trigger")));

        String targetVoiceSessionId = ttlUseCase.open(openCommand("s-ttl"));
        ttlUseCase.stop(stopCommand(targetVoiceSessionId));

        VoiceSessionUseCase.RecoverySnapshot beforeExpiry = ttlUseCase.recover(targetVoiceSessionId, 0L);
        assertTrue(beforeExpiry.stopped());

        now.addAndGet(600_001L);
        ttlUseCase.open(openCommand("s-trigger"));

        IllegalArgumentException error = assertThrows(
                IllegalArgumentException.class,
                () -> ttlUseCase.recover(targetVoiceSessionId, 0L)
        );
        assertEquals("Unknown voice session: " + targetVoiceSessionId, error.getMessage());
    }

    @Test
    void purgeStoppedContexts_enforcesStoppedContextLimitWithoutRemovingActive() {
        AtomicLong now = new AtomicLong(10_000L);
        VoiceSessionUseCase limitedUseCase = new VoiceSessionUseCase(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics,
                256,
                now::get
        );
        when(voicePolicy.voiceRecoveryRetentionMs()).thenReturn(10_000_000L);
        when(voicePolicy.voiceStoppedContextMax()).thenReturn(2);
        when(sessionUseCase.getOrCreate(anyString()))
                .thenAnswer(invocation -> new SessionState(SessionId.of(invocation.getArgument(0))));

        String activeVoiceSessionId = limitedUseCase.open(openCommand("s-active"));
        String stoppedA = limitedUseCase.open(openCommand("s-a"));
        limitedUseCase.stop(stopCommand(stoppedA));
        now.incrementAndGet();

        String stoppedB = limitedUseCase.open(openCommand("s-b"));
        limitedUseCase.stop(stopCommand(stoppedB));
        now.incrementAndGet();

        String stoppedC = limitedUseCase.open(openCommand("s-c"));
        limitedUseCase.stop(stopCommand(stoppedC));

        VoiceSessionUseCase.RecoverySnapshot activeSnapshot = limitedUseCase.recover(activeVoiceSessionId, 0L);
        assertTrue(activeSnapshot.active());
        assertFalse(activeSnapshot.stopped());

        IllegalArgumentException error = assertThrows(
                IllegalArgumentException.class,
                () -> limitedUseCase.recover(stoppedA, 0L)
        );
        assertEquals("Unknown voice session: " + stoppedA, error.getMessage());
        assertTrue(limitedUseCase.recover(stoppedB, 0L).stopped());
        assertTrue(limitedUseCase.recover(stoppedC, 0L).stopped());
    }

    private VoiceSessionOpenCommand openCommand(String sessionId) {
        VoiceSessionOpenCommand command = new VoiceSessionOpenCommand();
        command.setSessionId(sessionId);
        command.setApiKey("api-key");
        return command;
    }

    private VoiceAudioChunkCommand audioChunk(String voiceSessionId) {
        return audioChunk(voiceSessionId, 1L);
    }

    private VoiceAudioChunkCommand audioChunk(String voiceSessionId, Long sequence) {
        VoiceAudioChunkCommand command = new VoiceAudioChunkCommand();
        command.setVoiceSessionId(voiceSessionId);
        command.setPcm16Base64(Base64.getEncoder().encodeToString(new byte[]{0, 1, 2, 3}));
        command.setSampleRate(16000);
        command.setSequence(sequence);
        return command;
    }

    private VoiceSessionStopCommand stopCommand(String voiceSessionId) {
        VoiceSessionStopCommand command = new VoiceSessionStopCommand();
        command.setVoiceSessionId(voiceSessionId);
        return command;
    }

    private NextQuestion question(String id, String text, String groupId, String group) {
        NextQuestion next = new NextQuestion();
        next.setQuestionId(id);
        next.setText(text);
        next.setGroupId(groupId);
        next.setGroup(group);
        next.setQuestionType("OPEN");
        next.setSkipped(false);
        return next;
    }

    private Feedback sampleFeedback() {
        return Feedback.of(
                "summary",
                List.of("point-1", "point-2", "point-3"),
                "example",
                List.of()
        );
    }

    private VoiceEventSink capture(List<EventRecord> events) {
        return (event, payload) -> {
            events.add(new EventRecord(event, mapPayload(payload)));
            return true;
        };
    }

    private Map<String, Object> mapPayload(Object payload) {
        if (!(payload instanceof Map<?, ?> raw)) {
            return Map.of();
        }
        Map<String, Object> typed = new LinkedHashMap<>();
        for (Map.Entry<?, ?> entry : raw.entrySet()) {
            typed.put(String.valueOf(entry.getKey()), entry.getValue());
        }
        return typed;
    }

    private int firstIndex(List<EventRecord> events, Predicate<EventRecord> predicate) {
        for (int i = 0; i < events.size(); i += 1) {
            if (predicate.test(events.get(i))) {
                return i;
            }
        }
        return -1;
    }

    private boolean isTtsRole(EventRecord event, String role) {
        if (!"tts.audio".equals(event.type())) {
            return false;
        }
        Object value = event.payload().get("role");
        if (value == null) {
            value = event.payload().get("phase");
        }
        return role.equals(value);
    }

    private long ttsSequence(Map<String, Object> payload) {
        Object value = payload.get("sequence");
        if (value instanceof Number number) {
            return number.longValue();
        }
        return Long.parseLong(String.valueOf(value));
    }

    private long eventId(Map<String, Object> payload) {
        Object value = payload.get("eventId");
        if (value instanceof Number number) {
            return number.longValue();
        }
        return Long.parseLong(String.valueOf(value));
    }

    private record EventRecord(String type, Map<String, Object> payload) {
    }
}
