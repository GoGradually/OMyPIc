package me.go_gradually.omypic.application.stt.usecase;

import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.model.SttEventSink;
import me.go_gradually.omypic.application.stt.model.SttJob;
import me.go_gradually.omypic.application.stt.port.SttJobStorePort;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class SttJobUseCaseTest {

    @Mock
    private SttUseCase sttUseCase;
    @Mock
    private SessionStorePort sessionStore;
    @Mock
    private SttJobStorePort jobStore;
    @Mock
    private AsyncExecutor asyncExecutor;

    private SttJobUseCase useCase;

    private static SttCommand command(String sessionId) {
        SttCommand command = new SttCommand();
        command.setSessionId(sessionId);
        command.setFileBytes(new byte[]{1});
        command.setModel("gpt-4o-mini-transcribe");
        command.setApiKey("key");
        command.setTranslate(false);
        return command;
    }

    @BeforeEach
    void setUp() {
        useCase = new SttJobUseCase(sttUseCase, sessionStore, jobStore, asyncExecutor);
    }

    @Test
    void createJob_processesTranscription_andSendsPartialAndFinalEvents() {
        SttCommand command = command("s1");
        SttJob job = new SttJob("job-1", "s1");
        SttEventSink sink = org.mockito.Mockito.mock(SttEventSink.class);
        job.getSinks().add(sink);

        when(jobStore.create(anyString(), eq("s1"))).thenReturn(job);
        when(sttUseCase.transcribe(command)).thenReturn("abcdefghi");
        when(sink.send(anyString(), anyString())).thenReturn(true);
        SessionState state = new SessionState(SessionId.of("s1"));
        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(state);
        doAnswer(invocation -> {
            Runnable task = invocation.getArgument(0);
            task.run();
            return null;
        }).when(asyncExecutor).execute(any(Runnable.class));

        String jobId = useCase.createJob(command);

        assertNotNull(jobId);
        assertTrue(job.isDone());
        assertEquals("abcdefghi", job.getText());
        assertEquals(1, state.getSttSegments().size());
        assertEquals("abcdefghi", state.getSttSegments().get(0));
        verify(sink, times(3)).send(eq("partial"), anyString());
        verify(sink).send("final", "abcdefghi");
    }

    @Test
    void registerSink_sendsFinalImmediately_whenJobAlreadyDone() {
        SttJob job = new SttJob("job-1", "s1");
        job.setText("final-text");
        job.setDone(true);
        SttEventSink sink = org.mockito.Mockito.mock(SttEventSink.class);
        when(jobStore.get("job-1")).thenReturn(job);
        when(sink.send(anyString(), anyString())).thenReturn(true);

        useCase.registerSink("job-1", sink);

        assertTrue(job.getSinks().contains(sink));
        verify(sink).send("final", "final-text");
    }

    @Test
    void registerSink_throwsForUnknownJob() {
        when(jobStore.get("missing")).thenReturn(null);

        assertThrows(IllegalArgumentException.class,
                () -> useCase.registerSink("missing", org.mockito.Mockito.mock(SttEventSink.class)));
    }

    @Test
    void unregisterSink_ignoresUnknownJob() {
        when(jobStore.get("missing")).thenReturn(null);

        useCase.unregisterSink("missing", org.mockito.Mockito.mock(SttEventSink.class));
    }

    @Test
    void createJob_removesSink_whenSendReturnsFalse() {
        SttCommand command = command("s1");
        SttJob job = new SttJob("job-1", "s1");
        SttEventSink sink = org.mockito.Mockito.mock(SttEventSink.class);
        job.getSinks().add(sink);

        when(jobStore.create(anyString(), eq("s1"))).thenReturn(job);
        when(sttUseCase.transcribe(command)).thenReturn("abcdefghi");
        when(sink.send(anyString(), anyString())).thenReturn(false);
        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(new SessionState(SessionId.of("s1")));
        doAnswer(invocation -> {
            Runnable task = invocation.getArgument(0);
            task.run();
            return null;
        }).when(asyncExecutor).execute(any(Runnable.class));

        useCase.createJob(command);

        assertFalse(job.getSinks().contains(sink));
        verify(sink, times(1)).send(anyString(), anyString());
    }

    @Test
    void createJob_sendsErrorEvent_whenSttFails() {
        SttCommand command = command("s1");
        SttJob job = new SttJob("job-1", "s1");
        SttEventSink sink = org.mockito.Mockito.mock(SttEventSink.class);
        job.getSinks().add(sink);

        when(jobStore.create(anyString(), eq("s1"))).thenReturn(job);
        when(sttUseCase.transcribe(command)).thenThrow(new IllegalStateException("boom"));
        when(sink.send(anyString(), anyString())).thenReturn(true);
        doAnswer(invocation -> {
            Runnable task = invocation.getArgument(0);
            task.run();
            return null;
        }).when(asyncExecutor).execute(any(Runnable.class));

        useCase.createJob(command);

        assertTrue(job.isDone());
        assertEquals("boom", job.getError());
        verify(sink).send("error", "boom");
        verify(sessionStore, never()).getOrCreate(any());
    }

    @Test
    void createJob_schedulesProcessingOnAsyncExecutor() {
        SttCommand command = command("s1");
        SttJob job = new SttJob("job-1", "s1");

        when(jobStore.create(anyString(), eq("s1"))).thenReturn(job);
        ArgumentCaptor<Runnable> taskCaptor = ArgumentCaptor.forClass(Runnable.class);

        useCase.createJob(command);

        verify(asyncExecutor).execute(taskCaptor.capture());
        assertNotNull(taskCaptor.getValue());
    }
}
