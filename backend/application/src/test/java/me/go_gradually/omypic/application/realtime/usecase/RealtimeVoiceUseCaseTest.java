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
import me.go_gradually.omypic.application.tts.model.AudioSink;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import me.go_gradually.omypic.application.tts.usecase.TtsUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.BooleanSupplier;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class RealtimeVoiceUseCaseTest {

    @Mock
    private RealtimeAudioGateway realtimeAudioGateway;
    @Mock
    private FeedbackUseCase feedbackUseCase;
    @Mock
    private TtsUseCase ttsUseCase;
    @Mock
    private SessionUseCase sessionUseCase;
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
        when(realtimePolicy.realtimeSttModel()).thenReturn("gpt-4o-mini-transcribe");
        when(realtimePolicy.realtimeFeedbackProvider()).thenReturn("openai");
        when(realtimePolicy.realtimeFeedbackModel()).thenReturn("gpt-4o-mini");
        when(realtimePolicy.realtimeFeedbackLanguage()).thenReturn("ko");
        when(realtimePolicy.realtimeTtsVoice()).thenReturn("alloy");

        when(realtimeAudioGateway.open(any(), any())).thenAnswer(invocation -> {
            listener = invocation.getArgument(1);
            return realtimeAudioSession;
        });

        doAnswer(invocation -> {
            Runnable runnable = invocation.getArgument(0);
            runnable.run();
            return null;
        }).when(asyncExecutor).execute(any(Runnable.class));

        useCase = new RealtimeVoiceUseCase(
                realtimeAudioGateway,
                feedbackUseCase,
                ttsUseCase,
                sessionUseCase,
                asyncExecutor,
                realtimePolicy,
                metrics
        );
    }

    @Test
    void finalTranscript_runsFeedbackAndTtsPipeline() throws Exception {
        Feedback feedback = Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of("[rule] evidence"));
        when(feedbackUseCase.generateFeedback(anyString(), any())).thenReturn(FeedbackResult.generated(feedback));
        doAnswer(invocation -> {
            AudioSink sink = invocation.getArgument(2);
            sink.write("chunk".getBytes(StandardCharsets.UTF_8));
            return null;
        }).when(ttsUseCase).streamUntil(anyString(), any(TtsCommand.class), any(AudioSink.class), any(BooleanSupplier.class));

        List<String> eventTypes = new CopyOnWriteArrayList<>();
        RealtimeVoiceSession session = useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
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
        verify(feedbackUseCase).generateFeedback(eq("api-key"), captor.capture());
        assertEquals("openai", captor.getValue().getProvider());
        assertEquals("gpt-4o-mini", captor.getValue().getModel());
        assertEquals("ko", captor.getValue().getFeedbackLanguage());
        session.close();
    }

    @Test
    void sessionUpdate_overridesFeedbackAndVoiceDefaults() throws Exception {
        Feedback feedback = Feedback.of("summary", List.of("p1", "p2", "p3"), "example", List.of());
        when(feedbackUseCase.generateFeedback(anyString(), any())).thenReturn(FeedbackResult.generated(feedback));
        doAnswer(invocation -> {
            AudioSink sink = invocation.getArgument(2);
            sink.write("a".getBytes(StandardCharsets.UTF_8));
            return null;
        }).when(ttsUseCase).streamUntil(anyString(), any(TtsCommand.class), any(AudioSink.class), any(BooleanSupplier.class));

        List<String> eventTypes = new ArrayList<>();
        RealtimeVoiceSession session = useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
            return true;
        });

        RealtimeSessionUpdateCommand update = new RealtimeSessionUpdateCommand();
        update.setFeedbackProvider("gemini");
        update.setFeedbackModel("gemini-2.0-flash");
        update.setFeedbackApiKey("gemini-key");
        update.setFeedbackLanguage("en");
        update.setTtsVoice("echo");
        session.update(update);

        listener.onFinalTranscript("answer");

        ArgumentCaptor<FeedbackCommand> feedbackCaptor = ArgumentCaptor.forClass(FeedbackCommand.class);
        verify(feedbackUseCase).generateFeedback(eq("gemini-key"), feedbackCaptor.capture());
        assertEquals("gemini", feedbackCaptor.getValue().getProvider());
        assertEquals("gemini-2.0-flash", feedbackCaptor.getValue().getModel());
        assertEquals("en", feedbackCaptor.getValue().getFeedbackLanguage());

        ArgumentCaptor<TtsCommand> ttsCaptor = ArgumentCaptor.forClass(TtsCommand.class);
        verify(ttsUseCase).streamUntil(eq("api-key"), ttsCaptor.capture(), any(AudioSink.class), any(BooleanSupplier.class));
        assertEquals("echo", ttsCaptor.getValue().getVoice());
        assertTrue(eventTypes.contains("session.updated"));
        session.close();
    }

    @Test
    void appendAudio_triggersBargeInCancel() {
        when(feedbackUseCase.generateFeedback(anyString(), any())).thenReturn(FeedbackResult.skipped());
        List<String> eventTypes = new CopyOnWriteArrayList<>();
        RealtimeVoiceSession session = useCase.open(startCommand(), (event, payload) -> {
            eventTypes.add(event);
            return true;
        });

        listener.onFinalTranscript("first");
        session.appendAudio("YmFzZTY0");

        verify(realtimeAudioSession, atLeastOnce()).cancelResponse();
        verify(realtimeAudioSession).appendBase64Audio("YmFzZTY0");
        assertTrue(eventTypes.contains("response.cancelled"));
        session.close();
    }

    private RealtimeStartCommand startCommand() {
        RealtimeStartCommand command = new RealtimeStartCommand();
        command.setSessionId("s1");
        command.setApiKey("api-key");
        return command;
    }
}
