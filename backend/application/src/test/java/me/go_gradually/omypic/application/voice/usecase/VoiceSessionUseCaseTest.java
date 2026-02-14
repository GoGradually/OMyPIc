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
import java.util.function.Predicate;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyInt;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.doAnswer;
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

        doAnswer(invocation -> {
            Runnable task = invocation.getArgument(0);
            task.run();
            return null;
        }).when(asyncExecutor).execute(any(Runnable.class));

        when(sttUseCase.transcribe(any(SttCommand.class))).thenReturn("answer text");
        lenient().when(feedbackUseCase.generateFeedbackForTurn(
                anyString(),
                any(FeedbackCommand.class),
                anyString(),
                any(),
                anyString(),
                anyInt()
        )).thenReturn(sampleFeedback());
        when(ttsGateway.synthesize(anyString(), anyString(), anyString(), anyString()))
                .thenReturn(new byte[]{1, 2, 3, 4});
    }

    @Test
    void immediateMode_emitsFeedbackSpeechBeforeNextQuestionSpeech() {
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

    private record EventRecord(String type, Map<String, Object> payload) {
    }
}
