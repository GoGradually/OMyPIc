package me.go_gradually.omypic.application.stt.usecase.integration;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.stt.model.SttCommand;
import me.go_gradually.omypic.application.stt.model.SttEventSink;
import me.go_gradually.omypic.application.stt.model.SttJob;
import me.go_gradually.omypic.application.stt.model.VadSettings;
import me.go_gradually.omypic.application.stt.policy.SttPolicy;
import me.go_gradually.omypic.application.stt.port.SttGateway;
import me.go_gradually.omypic.application.stt.port.SttJobStorePort;
import me.go_gradually.omypic.application.stt.usecase.SttJobUseCase;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteId;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.time.Duration;
import java.util.*;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class VoiceChainIntegrationTest {

    @Mock
    private SttGateway sttGateway;
    @Mock
    private LlmClient llmClient;
    @Mock
    private RulebookUseCase rulebookUseCase;

    private static SttCommand sttCommand(String sessionId) {
        SttCommand command = new SttCommand();
        command.setSessionId(sessionId);
        command.setFileBytes(new byte[]{1, 2, 3});
        command.setApiKey("key");
        command.setModel("gpt-4o-mini-transcribe");
        command.setTranslate(false);
        return command;
    }

    private static FeedbackCommand feedbackCommand(String sessionId, String provider, String language, String text) {
        FeedbackCommand command = new FeedbackCommand();
        command.setSessionId(sessionId);
        command.setProvider(provider);
        command.setModel("model");
        command.setFeedbackLanguage(language);
        command.setText(text);
        return command;
    }

    @Test
    void voiceChain_success_sttToFeedbackToWrongNote() throws Exception {
        TestMetrics metrics = new TestMetrics();
        InMemorySessionStore sessionStore = new InMemorySessionStore();
        InMemorySttJobStore jobStore = new InMemorySttJobStore();
        ControlledExecutor executor = new ControlledExecutor();
        WrongNoteUseCase wrongNoteUseCase = new WrongNoteUseCase(
                new InMemoryWrongNotePort(),
                new InMemoryWrongNoteRecentQueuePort(),
                new FixedFeedbackPolicy()
        );

        when(sttGateway.transcribe(any(), anyString(), anyString(), org.mockito.ArgumentMatchers.anyBoolean(), any()))
                .thenReturn("This is my answer.");
        when(llmClient.provider()).thenReturn("openai");
        when(llmClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("{\"summary\":\"summary\",\"correctionPoints\":[\"Grammar: tense\"],\"exampleAnswer\":\"tiny\",\"rulebookEvidence\":[]}");
        when(rulebookUseCase.searchContexts(anyString()))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r1"), "rulebook.md", "keep tense consistent")));

        SttUseCase sttUseCase = new SttUseCase(sttGateway, new FixedSttPolicy(100, 2), metrics);
        SttJobUseCase sttJobUseCase = new SttJobUseCase(sttUseCase, sessionStore, jobStore, executor);
        FeedbackUseCase feedbackUseCase = new FeedbackUseCase(
                List.of(llmClient),
                rulebookUseCase,
                new FixedFeedbackPolicy(),
                metrics,
                sessionStore,
                wrongNoteUseCase
        );

        SttCommand sttCommand = sttCommand("s1");
        String jobId = sttJobUseCase.createJob(sttCommand);
        CapturingSink sink = new CapturingSink();
        sttJobUseCase.registerSink(jobId, sink);
        executor.runAll();

        assertTrue(sink.events.stream().anyMatch(event -> event.startsWith("partial:")));
        assertTrue(sink.events.stream().anyMatch(event -> event.equals("final:This is my answer.")));
        assertEquals(List.of("This is my answer."), sessionStore.getOrCreate(SessionId.of("s1")).getSttSegments());

        FeedbackResult result = feedbackUseCase.generateFeedback("key", feedbackCommand("s1", "openai", "en", "This is my answer."));

        assertTrue(result.isGenerated());
        assertFalse(wrongNoteUseCase.list().isEmpty());
        assertEquals(1, metrics.sttLatencyCalls);
        assertEquals(1, metrics.feedbackLatencyCalls);
    }

    @Test
    void sttJob_retriesAndSucceeds_withoutErrorMetric() throws Exception {
        TestMetrics metrics = new TestMetrics();
        InMemorySessionStore sessionStore = new InMemorySessionStore();
        InMemorySttJobStore jobStore = new InMemorySttJobStore();
        ControlledExecutor executor = new ControlledExecutor();

        when(sttGateway.transcribe(any(), anyString(), anyString(), org.mockito.ArgumentMatchers.anyBoolean(), any()))
                .thenThrow(new RuntimeException("temporary"))
                .thenReturn("retry success");

        SttUseCase sttUseCase = new SttUseCase(sttGateway, new FixedSttPolicy(100, 2), metrics);
        SttJobUseCase sttJobUseCase = new SttJobUseCase(sttUseCase, sessionStore, jobStore, executor);

        String jobId = sttJobUseCase.createJob(sttCommand("s2"));
        CapturingSink sink = new CapturingSink();
        sttJobUseCase.registerSink(jobId, sink);
        executor.runAll();

        assertTrue(sink.events.contains("final:retry success"));
        assertEquals(0, metrics.sttErrorCount);
        verify(sttGateway, times(2)).transcribe(any(), anyString(), anyString(), org.mockito.ArgumentMatchers.anyBoolean(), any());
    }

    @Test
    void sttJob_whenFailed_setsErrorAndSendsErrorEvent() throws Exception {
        TestMetrics metrics = new TestMetrics();
        InMemorySessionStore sessionStore = new InMemorySessionStore();
        InMemorySttJobStore jobStore = new InMemorySttJobStore();
        ControlledExecutor executor = new ControlledExecutor();

        when(sttGateway.transcribe(any(), anyString(), anyString(), org.mockito.ArgumentMatchers.anyBoolean(), any()))
                .thenThrow(new RuntimeException("fatal"));

        SttUseCase sttUseCase = new SttUseCase(sttGateway, new FixedSttPolicy(100, 0), metrics);
        SttJobUseCase sttJobUseCase = new SttJobUseCase(sttUseCase, sessionStore, jobStore, executor);

        String jobId = sttJobUseCase.createJob(sttCommand("s3"));
        CapturingSink sink = new CapturingSink();
        sttJobUseCase.registerSink(jobId, sink);
        executor.runAll();

        SttJob job = jobStore.get(jobId);
        assertTrue(job.isDone());
        assertEquals("STT failed", job.getError());
        assertTrue(sink.events.contains("error:STT failed"));
        assertEquals(1, metrics.sttErrorCount);
    }

    private record FixedSttPolicy(long maxBytes, int retryMax) implements SttPolicy {

        @Override
            public long getMaxFileBytes() {
                return maxBytes;
            }

            @Override
            public VadSettings getVadSettings() {
                return new VadSettings(300, 500, 0.6);
            }
        }

    private static class FixedFeedbackPolicy implements FeedbackPolicy {
        @Override
        public int getSummaryMaxChars() {
            return 255;
        }

        @Override
        public double getExampleMinRatio() {
            return 0.8;
        }

        @Override
        public double getExampleMaxRatio() {
            return 1.2;
        }

        @Override
        public int getWrongnoteSummaryMaxChars() {
            return 255;
        }
    }

    private static class TestMetrics implements MetricsPort {
        private int sttLatencyCalls;
        private int feedbackLatencyCalls;
        private int sttErrorCount;

        @Override
        public void recordSttLatency(Duration duration) {
            sttLatencyCalls += 1;
        }

        @Override
        public void recordFeedbackLatency(Duration duration) {
            feedbackLatencyCalls += 1;
        }

        @Override
        public void recordTtsLatency(Duration duration) {
        }

        @Override
        public void recordQuestionNextLatency(Duration duration) {
        }

        @Override
        public void incrementSttRequest() {
        }

        @Override
        public void incrementSttError() {
            sttErrorCount += 1;
        }

        @Override
        public void incrementFeedbackError() {
        }

        @Override
        public void incrementTtsError() {
        }

        @Override
        public void recordRealtimeTurnLatency(Duration duration) {
        }

        @Override
        public void recordRulebookUploadLatency(Duration duration) {
        }
    }

    private static class InMemorySessionStore implements SessionStorePort {
        private final Map<SessionId, SessionState> sessions = new HashMap<>();

        @Override
        public SessionState getOrCreate(SessionId sessionId) {
            return sessions.computeIfAbsent(sessionId, SessionState::new);
        }
    }

    private static class InMemoryWrongNotePort implements WrongNotePort {
        private final Map<String, WrongNote> byPattern = new LinkedHashMap<>();

        @Override
        public List<WrongNote> findAll() {
            return new ArrayList<>(byPattern.values());
        }

        @Override
        public Optional<WrongNote> findByPattern(String pattern) {
            return Optional.ofNullable(byPattern.get(pattern));
        }

        @Override
        public WrongNote save(WrongNote note) {
            byPattern.put(note.getPattern(), note);
            return note;
        }

        @Override
        public void deleteById(WrongNoteId id) {
            byPattern.entrySet().removeIf(entry -> entry.getValue().getId().equals(id));
        }
    }

    private static class InMemoryWrongNoteRecentQueuePort implements WrongNoteRecentQueuePort {
        private final List<String> queue = new ArrayList<>();

        @Override
        public List<String> loadGlobalQueue() {
            return List.copyOf(queue);
        }

        @Override
        public void saveGlobalQueue(List<String> patterns) {
            queue.clear();
            queue.addAll(patterns);
        }
    }

    private static class InMemorySttJobStore implements SttJobStorePort {
        private final Map<String, SttJob> storage = new HashMap<>();

        @Override
        public SttJob create(String jobId, String sessionId) {
            SttJob job = new SttJob(jobId, sessionId);
            storage.put(jobId, job);
            return job;
        }

        @Override
        public SttJob get(String jobId) {
            return storage.get(jobId);
        }
    }

    private static class ControlledExecutor implements AsyncExecutor {
        private final ArrayDeque<Runnable> queue = new ArrayDeque<>();

        @Override
        public void execute(Runnable task) {
            queue.add(task);
        }

        private void runAll() {
            while (!queue.isEmpty()) {
                queue.removeFirst().run();
            }
        }
    }

    private static class CapturingSink implements SttEventSink {
        private final List<String> events = new ArrayList<>();

        @Override
        public boolean send(String event, String data) {
            events.add(event + ":" + data);
            return true;
        }
    }
}
