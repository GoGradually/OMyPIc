package me.go_gradually.omypic.application.feedback.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.time.Instant;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class FeedbackUseCaseTest {

    @Mock
    private LlmClient openAiClient;
    @Mock
    private RulebookUseCase rulebookUseCase;
    @Mock
    private FeedbackPolicy feedbackPolicy;
    @Mock
    private MetricsPort metrics;
    @Mock
    private SessionStorePort sessionStore;
    @Mock
    private WrongNoteUseCase wrongNoteUseCase;

    private FeedbackUseCase useCase;

    private static FeedbackCommand command(String sessionId, String provider, String language, String text) {
        FeedbackCommand command = new FeedbackCommand();
        command.setSessionId(sessionId);
        command.setProvider(provider);
        command.setModel("model");
        command.setFeedbackLanguage(language);
        command.setText(text);
        return command;
    }

    @BeforeEach
    void setUp() {
        when(openAiClient.provider()).thenReturn("openai");
        useCase = new FeedbackUseCase(
                List.of(openAiClient),
                rulebookUseCase,
                feedbackPolicy,
                metrics,
                sessionStore,
                wrongNoteUseCase
        );
    }

    @Test
    void generateFeedback_skipsInMockExamMode_withoutCallingLlm() throws Exception {
        SessionState state = new SessionState(SessionId.of("s1"));
        state.applyModeUpdate(ModeType.MOCK_EXAM, null);
        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(state);

        FeedbackResult result = useCase.generateFeedback("key", command("s1", "openai", "en", "hello"));

        assertFalse(result.isGenerated());
        verify(openAiClient, never()).generate(anyString(), anyString(), anyString(), anyString());
        verify(wrongNoteUseCase, never()).addFeedback(any());
    }

    @Test
    void generateFeedback_normalizesRequiredFields_andRecordsMetrics() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("s2"));
        when(sessionStore.getOrCreate(SessionId.of("s2"))).thenReturn(state);
        when(rulebookUseCase.searchContexts("This is my answer."))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r1"), "rulebook.md", "always include evidence")));
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("```json\n{\"summary\":\"summary\",\"correctionPoints\":[\"Grammar: tense\"],\"exampleAnswer\":\"tiny\",\"rulebookEvidence\":[]}\n```");

        FeedbackResult result = useCase.generateFeedback("key", command("s2", "OpenAI", "en", "This is my answer."));

        assertTrue(result.isGenerated());
        Feedback feedback = result.getFeedback();
        assertEquals(3, feedback.getCorrectionPoints().size());
        assertEquals(1, feedback.getRulebookEvidence().size());
        assertTrue(feedback.getRulebookEvidence().get(0).startsWith("[rulebook.md]"));
        assertTrue(feedback.getExampleAnswer().length() >= 14);
        assertTrue(feedback.getExampleAnswer().length() <= 21);
        assertEquals("en", state.getFeedbackLanguage().value());
        verify(metrics).recordFeedbackLatency(any());
        verify(metrics, never()).incrementFeedbackError();
        verify(wrongNoteUseCase).addFeedback(any(Feedback.class));
    }

    @Test
    void generateFeedback_inContinuousMode_usesMostRecentBatchText() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("s-batch"));
        state.applyModeUpdate(ModeType.CONTINUOUS, 3);
        when(sessionStore.getOrCreate(SessionId.of("s-batch"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("{\"summary\":\"summary\",\"correctionPoints\":[\"Grammar\",\"Expression\",\"Logic\"],\"exampleAnswer\":\"example answer\",\"rulebookEvidence\":[]}");

        state.appendSegment("first answer");
        FeedbackResult first = useCase.generateFeedback("key", command("s-batch", "openai", "en", "first answer"));
        state.appendSegment("second answer");
        FeedbackResult second = useCase.generateFeedback("key", command("s-batch", "openai", "en", "second answer"));
        state.appendSegment("third answer");
        FeedbackResult third = useCase.generateFeedback("key", command("s-batch", "openai", "en", "third answer"));

        assertFalse(first.isGenerated());
        assertFalse(second.isGenerated());
        assertTrue(third.isGenerated());
        verify(rulebookUseCase).searchContexts("first answer\nsecond answer\nthird answer");
    }

    @Test
    void generateFeedback_usesDefaultKoreanLanguage_whenCommandLanguageIsNull() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("s3"));
        when(sessionStore.getOrCreate(SessionId.of("s3"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("{\"summary\":\"요약\",\"correctionPoints\":[\"Grammar\",\"Expression\",\"Logic\"],\"exampleAnswer\":\"예시 답변입니다\",\"rulebookEvidence\":[]}");

        FeedbackCommand command = command("s3", "openai", null, "짧은 답변");
        FeedbackResult result = useCase.generateFeedback("key", command);

        assertTrue(result.isGenerated());
        assertEquals("ko", state.getFeedbackLanguage().value());
    }

    @Test
    void generateFeedback_clearsEvidenceWhenNoRulebookContext() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("s4"));
        when(sessionStore.getOrCreate(SessionId.of("s4"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("{\"summary\":\"s\",\"correctionPoints\":[\"Grammar\",\"Expression\",\"Logic\"],\"exampleAnswer\":\"123456789\",\"rulebookEvidence\":[\"should be removed\"]}");

        FeedbackResult result = useCase.generateFeedback("key", command("s4", "openai", "ko", "123456789"));

        assertTrue(result.getFeedback().getRulebookEvidence().isEmpty());
    }

    @Test
    void generateFeedback_throwsAndIncrementsError_whenJsonIsMalformed() throws Exception {
        SessionState state = new SessionState(SessionId.of("s5"));
        when(sessionStore.getOrCreate(SessionId.of("s5"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("not-json");

        assertThrows(IllegalStateException.class,
                () -> useCase.generateFeedback("key", command("s5", "openai", "ko", "답변")));

        verify(metrics).incrementFeedbackError();
        verify(wrongNoteUseCase, never()).addFeedback(any());
    }

    @Test
    void generateFeedback_throwsForUnknownProvider_withoutIncrementingErrorMetric() {
        SessionState state = new SessionState(SessionId.of("s6"));
        when(sessionStore.getOrCreate(SessionId.of("s6"))).thenReturn(state);

        assertThrows(IllegalArgumentException.class,
                () -> useCase.generateFeedback("key", command("s6", "claude", "ko", "답변")));

        verify(metrics, never()).incrementFeedbackError();
    }

    @Test
    void generateMockExamFinalFeedback_generatesOnceAfterCompletion() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("mock-1"));
        state.applyModeUpdate(ModeType.MOCK_EXAM, null);
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "mock",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", QuestionGroup.of("A"))),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        state.configureMockExam(list, List.of(QuestionGroup.of("A")), java.util.Map.of(QuestionGroup.of("A"), 1));
        state.nextQuestion(list);
        state.nextQuestion(list);
        state.appendSegment("mock answer one");
        state.appendSegment("mock answer two");

        when(sessionStore.getOrCreate(SessionId.of("mock-1"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("{\"summary\":\"summary\",\"correctionPoints\":[\"Grammar\",\"Expression\",\"Logic\"],\"exampleAnswer\":\"example answer\",\"rulebookEvidence\":[]}");

        FeedbackResult result = useCase.generateMockExamFinalFeedback("key", command("mock-1", "openai", "ko", "ignored"));

        assertTrue(result.isGenerated());
        assertTrue(state.isMockFinalFeedbackGenerated());
        verify(rulebookUseCase).searchContexts("mock answer one\nmock answer two");

        IllegalStateException exception = assertThrows(IllegalStateException.class,
                () -> useCase.generateMockExamFinalFeedback("key", command("mock-1", "openai", "ko", "ignored")));
        assertTrue(exception.getMessage().contains("already generated"));
    }

    @Test
    void generateFeedbackForTurn_usesQuestionGroupScopedContexts() throws Exception {
        stubDefaultFeedbackPolicy();
        when(rulebookUseCase.searchContextsForTurn(QuestionGroup.of("A"), "Question text\nAnswer text", 2))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r1"), "a.md", "group A rules")));
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn("{\"summary\":\"summary\",\"correctionPoints\":[\"Grammar\",\"Expression\",\"Logic\"],\"exampleAnswer\":\"example answer\",\"rulebookEvidence\":[]}");

        Feedback feedback = useCase.generateFeedbackForTurn(
                "key",
                command("s-turn", "openai", "en", "ignored"),
                "Question text",
                QuestionGroup.of("A"),
                "Answer text",
                2
        );

        assertNotNull(feedback);
        verify(rulebookUseCase).searchContextsForTurn(QuestionGroup.of("A"), "Question text\nAnswer text", 2);
        verify(wrongNoteUseCase).addFeedback(any(Feedback.class));
    }

    private void stubDefaultFeedbackPolicy() {
        when(feedbackPolicy.getSummaryMaxChars()).thenReturn(255);
        when(feedbackPolicy.getExampleMinRatio()).thenReturn(0.8);
        when(feedbackPolicy.getExampleMaxRatio()).thenReturn(1.2);
    }
}
