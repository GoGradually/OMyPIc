package me.go_gradually.omypic.application.voice.usecase;

import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.application.voice.model.VoiceSessionOpenCommand;
import me.go_gradually.omypic.application.voice.policy.VoicePolicy;
import me.go_gradually.omypic.application.voice.port.TtsGateway;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

@ExtendWith(MockitoExtension.class)
class VoiceSessionUseCaseValidationTest {
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

    @Test
    void open_throws_whenFeedbackModelIsUnsupported() {
        VoiceSessionUseCase useCase = new VoiceSessionUseCase(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics
        );
        VoiceSessionOpenCommand command = new VoiceSessionOpenCommand();
        command.setSessionId("s1");
        command.setApiKey("api-key");
        command.setFeedbackModel("gpt-5.2");

        IllegalArgumentException error = assertThrows(IllegalArgumentException.class, () -> useCase.open(command));

        assertEquals("Unsupported feedback model: gpt-5.2", error.getMessage());
    }
}
