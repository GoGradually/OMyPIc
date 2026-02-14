package me.go_gradually.omypic.application.feedback.usecase;

import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.domain.feedback.CorrectionDetail;
import me.go_gradually.omypic.domain.feedback.Corrections;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.RecommendationDetail;
import me.go_gradually.omypic.domain.feedback.Recommendations;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.List;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.ArgumentMatchers.contains;
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
    void generateFeedback_normalizesRequiredFields_andRecordsMetrics() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("s2"));
        when(sessionStore.getOrCreate(SessionId.of("s2"))).thenReturn(state);
        when(rulebookUseCase.searchContexts("This is my answer."))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r1"), "rulebook.md", "always include evidence")));
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn(successResult());

        FeedbackResult result = useCase.generateFeedback("key", command("s2", "OpenAI", "en", "This is my answer."));

        assertTrue(result.isGenerated());
        Feedback feedback = result.getFeedback();
        assertFalse(feedback.getCorrections().grammar().issue().isBlank());
        assertFalse(feedback.getRecommendations().filler().term().isBlank());
        assertEquals(1, feedback.getRulebookEvidence().size());
        assertTrue(feedback.getRulebookEvidence().get(0).startsWith("[rulebook.md]"));
        assertTrue(feedback.getExampleAnswer().length() >= 14);
        assertTrue(feedback.getExampleAnswer().length() <= 21);
        assertEquals("en", state.getFeedbackLanguage().value());
        verify(metrics).recordFeedbackLatency(any());
        verify(metrics, never()).incrementFeedbackSchemaFallback();
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
                .thenReturn(successResult());

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
                .thenReturn(successResult());

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
                .thenReturn(new LlmGenerateResult(
                        Feedback.of(
                                "s",
                                new Corrections(
                                        new CorrectionDetail("issue", "fix"),
                                        new CorrectionDetail("issue", "fix"),
                                        new CorrectionDetail("issue", "fix")
                                ),
                                new Recommendations(
                                        new RecommendationDetail("Well", "usage"),
                                        new RecommendationDetail("vivid", "usage"),
                                        new RecommendationDetail("definitely", "usage")
                                ),
                                "123456789",
                                List.of("should be removed")
                        ),
                        List.of()
                ));

        FeedbackResult result = useCase.generateFeedback("key", command("s4", "openai", "ko", "123456789"));

        assertTrue(result.getFeedback().getRulebookEvidence().isEmpty());
    }

    @Test
    void generateFeedback_incrementsSchemaFallbackMetric_whenClientReportsFallback() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("s4-fallback"));
        when(sessionStore.getOrCreate(SessionId.of("s4-fallback"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn(new LlmGenerateResult(successResult().feedback(), List.of("structured_output_conversion_failed")));

        FeedbackResult result = useCase.generateFeedback("key", command("s4-fallback", "openai", "en", "answer text"));

        assertTrue(result.isGenerated());
        verify(metrics).incrementFeedbackSchemaFallback();
    }

    @Test
    void generateFeedback_systemPrompt_enforcesStructuredContract() throws Exception {
        stubDefaultFeedbackPolicy();
        SessionState state = new SessionState(SessionId.of("s4-prompt"));
        when(sessionStore.getOrCreate(SessionId.of("s4-prompt"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn(successResult());

        FeedbackResult result = useCase.generateFeedback("key", command("s4-prompt", "openai", "en", "answer text"));

        assertTrue(result.isGenerated());
        ArgumentCaptor<String> systemPromptCaptor = ArgumentCaptor.forClass(String.class);
        verify(openAiClient).generate(anyString(), anyString(), systemPromptCaptor.capture(), anyString());
        String systemPrompt = systemPromptCaptor.getValue();
        assertTrue(systemPrompt.contains("summary, corrections, recommendations, exampleAnswer, rulebookEvidence"));
        assertTrue(systemPrompt.contains("\"grammar\": {\"issue\": \"string\", \"fix\": \"string\"}"));
        assertTrue(systemPrompt.contains("\"filler\": {\"term\": \"string\", \"usage\": \"string\"}"));
    }

    @Test
    void generateFeedback_throwsAndIncrementsError_whenClientFails() throws Exception {
        SessionState state = new SessionState(SessionId.of("s5"));
        when(sessionStore.getOrCreate(SessionId.of("s5"))).thenReturn(state);
        when(rulebookUseCase.searchContexts(anyString())).thenReturn(List.of());
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenThrow(new IllegalStateException("bad response"));

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
    void generateFeedbackForTurn_usesQuestionGroupScopedContexts() throws Exception {
        stubDefaultFeedbackPolicy();
        when(rulebookUseCase.searchContextsForTurn(QuestionGroup.of("A"), "Question text\nAnswer text", 2))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r1"), "a.md", "group A rules")));
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn(successResult());

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

    @Test
    void generateFeedbackForTurnWithPrefetch_reusesPrefetchedPromptWithoutExtraSearch() throws Exception {
        stubDefaultFeedbackPolicy();
        when(rulebookUseCase.searchContextsForTurn(QuestionGroup.of("A"), "Question text", 2))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r1"), "a.md", "group A rules")));
        when(openAiClient.generate(anyString(), anyString(), anyString(), anyString()))
                .thenReturn(successResult());

        FeedbackUseCase.PrefetchedTurnPrompt prefetch = useCase.prefetchTurnPrompt(
                "q-1",
                "Question text",
                QuestionGroup.of("A"),
                "en",
                2
        );
        Feedback feedback = useCase.generateFeedbackForTurnWithPrefetch(
                "key",
                command("s-turn", "openai", "en", "ignored"),
                "Answer text",
                prefetch
        );

        assertNotNull(feedback);
        verify(rulebookUseCase, times(1)).searchContextsForTurn(QuestionGroup.of("A"), "Question text", 2);
        verify(openAiClient).generate(anyString(), anyString(), anyString(), contains("Answer text"));
        verify(wrongNoteUseCase).addFeedback(any(Feedback.class));
    }

    private LlmGenerateResult successResult() {
        Feedback feedback = Feedback.of(
                "summary",
                new Corrections(
                        new CorrectionDetail("tense", "use past tense"),
                        new CorrectionDetail("wording", "use specific vocabulary"),
                        new CorrectionDetail("reason", "add one reason")
                ),
                new Recommendations(
                        new RecommendationDetail("Well", "Use this to start naturally."),
                        new RecommendationDetail("impressive", "Use it for vivid detail."),
                        new RecommendationDetail("definitely", "Use it for confidence.")
                ),
                "tiny",
                List.of()
        );
        return new LlmGenerateResult(feedback, List.of());
    }

    private void stubDefaultFeedbackPolicy() {
        when(feedbackPolicy.getSummaryMaxChars()).thenReturn(255);
        when(feedbackPolicy.getExampleMinRatio()).thenReturn(0.8);
        when(feedbackPolicy.getExampleMaxRatio()).thenReturn(1.2);
    }
}
