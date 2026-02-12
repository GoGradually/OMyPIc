package me.go_gradually.omypic.application.realtime.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.application.realtime.model.*;
import me.go_gradually.omypic.application.realtime.policy.RealtimePolicy;
import me.go_gradually.omypic.application.realtime.port.RealtimeAudioGateway;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CopyOnWriteArrayList;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyInt;
import static org.mockito.ArgumentMatchers.anyLong;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class RealtimeVoiceUseCaseTest {

    @Mock
    private RealtimeAudioGateway realtimeAudioGateway;
    @Mock
    private FeedbackUseCase feedbackUseCase;
    @Mock
    private SessionUseCase sessionUseCase;
    @Mock
    private QuestionUseCase questionUseCase;
    @Mock
    private AsyncExecutor asyncExecutor;
    @Mock
    private MetricsPort metrics;
    @Mock
    private RealtimePolicy realtimePolicy;
    @Mock
    private RealtimeAudioSession realtimeAudioSession;

    private RealtimeVoiceUseCase useCase;
    private RealtimeAudioEventListener listener;

    @BeforeEach
    void setUp() {
        SessionState sessionState = new SessionState(SessionId.of("s1"));
        sessionState.setActiveQuestionListId("list-1");
        when(sessionUseCase.getOrCreate("s1")).thenReturn(sessionState);

        lenient().when(questionUseCase.nextQuestion("list-1", "s1")).thenAnswer(invocation -> {
            me.go_gradually.omypic.application.question.model.NextQuestion next = new me.go_gradually.omypic.application.question.model.NextQuestion();
            next.setQuestionId("q-1");
            next.setText("first question");
            next.setGroup("A");
            return next;
        });

        when(realtimePolicy.realtimeConversationModel()).thenReturn("gpt-realtime-mini");
        when(realtimePolicy.realtimeSttModel()).thenReturn("gpt-4o-mini-transcribe");
        when(realtimePolicy.realtimeFeedbackProvider()).thenReturn("openai");
        when(realtimePolicy.realtimeFeedbackModel()).thenReturn("gpt-realtime-mini");
        when(realtimePolicy.realtimeFeedbackLanguage()).thenReturn("ko");
        when(realtimePolicy.realtimeTtsVoice()).thenReturn("alloy");

        when(realtimeAudioGateway.open(any(), any())).thenAnswer(invocation -> {
            listener = invocation.getArgument(1);
            return realtimeAudioSession;
        });

        useCase = new RealtimeVoiceUseCase(
                realtimeAudioGateway,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                asyncExecutor,
                realtimePolicy,
                metrics
        );
    }

    @Test
    void finalTranscript_runsFeedbackAndRealtimeSpeechPipeline() {
        stubAsyncExecutorRunsInline();
        stubRealtimeSpeechSuccess();
        Feedback feedback = Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of("[rule] evidence"));
        when(feedbackUseCase.generateFeedbackForTurn(anyString(), any(), anyString(), any(QuestionGroup.class), anyString(), anyInt())).thenReturn(feedback);

        List<String> eventTypes = new CopyOnWriteArrayList<>();
        List<Map<String, Object>> feedbackPayloads = new CopyOnWriteArrayList<>();
        RealtimeVoiceSession session = useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
            if ("feedback.final".equals(event) && payload instanceof Map<?, ?> map) {
                @SuppressWarnings("unchecked")
                Map<String, Object> typed = (Map<String, Object>) map;
                feedbackPayloads.add(typed);
            }
            return true;
        });

        listener.onPartialTranscript("hel");
        listener.onFinalTranscript("hello world");

        assertTrue(eventTypes.contains("connection.ready"));
        assertTrue(eventTypes.contains("stt.partial"));
        assertTrue(eventTypes.contains("stt.final"));
        assertTrue(eventTypes.contains("feedback.final"));
        assertTrue(eventTypes.contains("tts.chunk"));
        assertTrue(eventTypes.contains("turn.completed"));
        verify(sessionUseCase).appendSegment("s1", "hello world");
        verify(metrics).recordRealtimeTurnLatency(any());

        ArgumentCaptor<FeedbackCommand> captor = ArgumentCaptor.forClass(FeedbackCommand.class);
        verify(feedbackUseCase).generateFeedbackForTurn(eq("api-key"), captor.capture(), anyString(), any(QuestionGroup.class), anyString(), eq(2));
        assertEquals("openai", captor.getValue().getProvider());
        assertEquals("gpt-realtime-mini", captor.getValue().getModel());
        assertEquals("ko", captor.getValue().getFeedbackLanguage());

        Map<String, Object> firstFeedback = feedbackPayloads.get(0);
        assertTrue(firstFeedback.containsKey("policy"));
        assertTrue(firstFeedback.containsKey("batch"));
        assertTrue(firstFeedback.containsKey("nextAction"));
        @SuppressWarnings("unchecked")
        Map<String, Object> policy = (Map<String, Object>) firstFeedback.get("policy");
        assertEquals("IMMEDIATE", policy.get("mode"));

        ArgumentCaptor<RealtimeAudioOpenCommand> openCaptor = ArgumentCaptor.forClass(RealtimeAudioOpenCommand.class);
        verify(realtimeAudioGateway).open(openCaptor.capture(), any());
        assertEquals("gpt-realtime-mini", openCaptor.getValue().conversationModel());
        assertEquals("gpt-4o-mini-transcribe", openCaptor.getValue().sttModel());
        session.close();
    }

    @Test
    void sessionUpdate_overridesFeedbackAndVoiceDefaults() {
        stubAsyncExecutorRunsInline();
        stubRealtimeSpeechSuccess();
        Feedback feedback = Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of());
        when(feedbackUseCase.generateFeedbackForTurn(anyString(), any(), anyString(), any(QuestionGroup.class), anyString(), anyInt())).thenReturn(feedback);

        List<String> eventTypes = new ArrayList<>();
        RealtimeVoiceSession session = useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
            return true;
        });

        RealtimeSessionUpdateCommand update = new RealtimeSessionUpdateCommand();
        update.setConversationModel("gpt-realtime");
        update.setSttModel("gpt-4o-transcribe");
        update.setFeedbackProvider("gemini");
        update.setFeedbackModel("gemini-2.0-flash");
        update.setFeedbackApiKey("gemini-key");
        update.setFeedbackLanguage("en");
        update.setTtsVoice("echo");
        session.update(update);

        listener.onFinalTranscript("answer");

        ArgumentCaptor<FeedbackCommand> feedbackCaptor = ArgumentCaptor.forClass(FeedbackCommand.class);
        verify(feedbackUseCase).generateFeedbackForTurn(eq("gemini-key"), feedbackCaptor.capture(), anyString(), any(QuestionGroup.class), anyString(), eq(2));
        assertEquals("gemini", feedbackCaptor.getValue().getProvider());
        assertEquals("gemini-2.0-flash", feedbackCaptor.getValue().getModel());
        assertEquals("en", feedbackCaptor.getValue().getFeedbackLanguage());

        ArgumentCaptor<String> voiceCaptor = ArgumentCaptor.forClass(String.class);
        verify(realtimeAudioSession, atLeastOnce()).speakText(anyLong(), anyString(), voiceCaptor.capture());
        assertTrue(voiceCaptor.getAllValues().contains("echo"));
        assertTrue(eventTypes.contains("session.updated"));
        session.close();
    }

    @Test
    void open_usesRealtimeModelsFromStartCommandWhenProvided() {
        RealtimeStartCommand command = startCommand();
        command.setConversationModel("gpt-realtime");
        command.setSttModel("gpt-4o-transcribe");

        RealtimeVoiceSession session = useCase.open(command, (event, payload) -> true);

        ArgumentCaptor<RealtimeAudioOpenCommand> openCaptor = ArgumentCaptor.forClass(RealtimeAudioOpenCommand.class);
        verify(realtimeAudioGateway).open(openCaptor.capture(), any());
        assertEquals("gpt-realtime", openCaptor.getValue().conversationModel());
        assertEquals("gpt-4o-transcribe", openCaptor.getValue().sttModel());
        session.close();
    }

    @Test
    void appendAudio_doesNotAutoCancelResponse() {
        stubAsyncExecutorRunsInline();
        stubRealtimeSpeechSuccess();
        when(feedbackUseCase.generateFeedbackForTurn(anyString(), any(), anyString(), any(QuestionGroup.class), anyString(), anyInt()))
                .thenReturn(Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of()));
        List<String> eventTypes = new CopyOnWriteArrayList<>();
        RealtimeVoiceSession session = useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
            return true;
        });

        listener.onFinalTranscript("first");
        session.appendAudio("YmFzZTY0");

        verify(realtimeAudioSession, never()).cancelResponse();
        verify(realtimeAudioSession).appendBase64Audio("YmFzZTY0");
        assertTrue(eventTypes.stream().noneMatch("response.cancelled"::equals));
        session.close();
    }

    @Test
    void realtimeSpeechFailure_emitsTtsErrorAndStillCompletesTurn() {
        stubAsyncExecutorRunsInline();
        doAnswer(invocation -> {
            long turnId = invocation.getArgument(0);
            listener.onAssistantAudioFailed(turnId, "Realtime response failed");
            return null;
        }).when(realtimeAudioSession).speakText(anyLong(), anyString(), anyString());
        Feedback feedback = Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of());
        when(feedbackUseCase.generateFeedbackForTurn(anyString(), any(), anyString(), any(QuestionGroup.class), anyString(), anyInt())).thenReturn(feedback);

        List<String> eventTypes = new CopyOnWriteArrayList<>();
        RealtimeVoiceSession session = useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
            return true;
        });

        listener.onFinalTranscript("hello world");

        assertTrue(eventTypes.contains("tts.error"));
        assertTrue(eventTypes.contains("turn.completed"));
        session.close();
    }

    @Test
    void autoStopsSessionWhenQuestionsAreExhausted() {
        stubAsyncExecutorRunsInline();
        stubRealtimeSpeechSuccess();
        Feedback feedback = Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of());
        when(feedbackUseCase.generateFeedbackForTurn(anyString(), any(), anyString(), any(QuestionGroup.class), anyString(), anyInt())).thenReturn(feedback);
        when(questionUseCase.nextQuestion("list-1", "s1"))
                .thenReturn(question("q-1", "first question", "A"))
                .thenReturn(skippedQuestion());

        List<String> eventTypes = new CopyOnWriteArrayList<>();
        List<Map<String, Object>> stoppedPayloads = new CopyOnWriteArrayList<>();
        useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
            if ("session.stopped".equals(event) && payload instanceof Map<?, ?> map) {
                @SuppressWarnings("unchecked")
                Map<String, Object> typed = (Map<String, Object>) map;
                stoppedPayloads.add(typed);
            }
            return true;
        });

        listener.onFinalTranscript("answer");

        assertTrue(eventTypes.contains("session.stopped"));
        assertEquals("QUESTION_EXHAUSTED", stoppedPayloads.get(0).get("reason"));
        verify(realtimeAudioSession, atLeastOnce()).close();
    }

    private RealtimeStartCommand startCommand() {
        RealtimeStartCommand command = new RealtimeStartCommand();
        command.setSessionId("s1");
        command.setApiKey("api-key");
        return command;
    }

    private me.go_gradually.omypic.application.question.model.NextQuestion question(String questionId, String text, String group) {
        me.go_gradually.omypic.application.question.model.NextQuestion next =
                new me.go_gradually.omypic.application.question.model.NextQuestion();
        next.setQuestionId(questionId);
        next.setText(text);
        next.setGroup(group);
        return next;
    }

    private me.go_gradually.omypic.application.question.model.NextQuestion skippedQuestion() {
        return me.go_gradually.omypic.application.question.model.NextQuestion.skipped();
    }

    private void stubAsyncExecutorRunsInline() {
        doAnswer(invocation -> {
            Runnable runnable = invocation.getArgument(0);
            runnable.run();
            return null;
        }).when(asyncExecutor).execute(any(Runnable.class));
    }

    private void stubRealtimeSpeechSuccess() {
        lenient().doAnswer(invocation -> {
            long turnId = invocation.getArgument(0);
            listener.onAssistantAudioChunk(turnId, "Y2h1bms=");
            listener.onAssistantAudioCompleted(turnId);
            return null;
        }).when(realtimeAudioSession).speakText(anyLong(), anyString(), anyString());
    }
}
