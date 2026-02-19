package me.go_gradually.omypic.bootstrap;

import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;
import me.go_gradually.omypic.application.apikey.usecase.ApiKeyVerifyUseCase;
import me.go_gradually.omypic.application.datatransfer.usecase.DataTransferUseCase;
import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.application.rulebook.policy.RagPolicy;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.stt.policy.SttPolicy;
import me.go_gradually.omypic.application.stt.port.SttGateway;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.application.voice.policy.VoicePolicy;
import me.go_gradually.omypic.application.voice.port.TtsGateway;
import me.go_gradually.omypic.application.voice.usecase.VoiceSessionUseCase;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

@Configuration
public class UseCaseConfig {
    @Bean(destroyMethod = "shutdown")
    public ExecutorService sttExecutorService() {
        return Executors.newCachedThreadPool();
    }

    @Bean
    public AsyncExecutor asyncExecutor(ExecutorService sttExecutorService) {
        return sttExecutorService::execute;
    }

    @Bean
    public QuestionUseCase questionUseCase(QuestionGroupPort questionGroupPort,
                                           SessionStorePort sessionStore,
                                           MetricsPort metricsPort) {
        return new QuestionUseCase(questionGroupPort, sessionStore, metricsPort);
    }

    @Bean
    public SessionUseCase sessionUseCase(SessionStorePort sessionStore,
                                         QuestionGroupPort questionGroupPort) {
        return new SessionUseCase(sessionStore, questionGroupPort);
    }

    @Bean
    public RulebookUseCase rulebookUseCase(RulebookPort rulebookPort,
                                           RulebookIndexPort rulebookIndexPort,
                                           RulebookFileStore rulebookFileStore,
                                           RagPolicy ragPolicy,
                                           MetricsPort metricsPort) {
        return new RulebookUseCase(rulebookPort, rulebookIndexPort, rulebookFileStore, ragPolicy, metricsPort);
    }

    @Bean
    public WrongNoteUseCase wrongNoteUseCase(WrongNotePort wrongNotePort,
                                             WrongNoteRecentQueuePort wrongNoteRecentQueuePort,
                                             FeedbackPolicy feedbackPolicy) {
        return new WrongNoteUseCase(wrongNotePort, wrongNoteRecentQueuePort, feedbackPolicy);
    }

    @Bean
    public FeedbackUseCase feedbackUseCase(List<LlmClient> llmClients,
                                           RulebookUseCase rulebookUseCase,
                                           FeedbackPolicy feedbackPolicy,
                                           MetricsPort metricsPort,
                                           SessionStorePort sessionStore,
                                           WrongNoteUseCase wrongNoteUseCase,
                                           AppProperties properties) {
        return new FeedbackUseCase(
                llmClients,
                rulebookUseCase,
                feedbackPolicy,
                metricsPort,
                sessionStore,
                wrongNoteUseCase,
                properties.getIntegrations().getOpenai().getConversationRebaseTurns()
        );
    }

    @Bean
    public SttUseCase sttUseCase(SttGateway sttGateway,
                                 SttPolicy sttPolicy,
                                 MetricsPort metricsPort) {
        return new SttUseCase(sttGateway, sttPolicy, metricsPort);
    }

    @Bean
    public VoiceSessionUseCase voiceSessionUseCase(SttUseCase sttUseCase,
                                                   FeedbackUseCase feedbackUseCase,
                                                   SessionUseCase sessionUseCase,
                                                   QuestionUseCase questionUseCase,
                                                   TtsGateway ttsGateway,
                                                   AsyncExecutor asyncExecutor,
                                                   VoicePolicy voicePolicy,
                                                   MetricsPort metricsPort) {
        return new VoiceSessionUseCase(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metricsPort
        );
    }

    @Bean
    public ApiKeyVerifyUseCase apiKeyVerifyUseCase(ApiKeyProbePort probePort) {
        return new ApiKeyVerifyUseCase(probePort);
    }

    @Bean
    public DataTransferUseCase dataTransferUseCase(QuestionGroupPort questionGroupPort,
                                                   RulebookPort rulebookPort,
                                                   WrongNotePort wrongNotePort,
                                                   WrongNoteRecentQueuePort wrongNoteRecentQueuePort,
                                                   RulebookFileStore rulebookFileStore,
                                                   RulebookIndexPort rulebookIndexPort,
                                                   ObjectMapper objectMapper) {
        return new DataTransferUseCase(
                questionGroupPort,
                rulebookPort,
                wrongNotePort,
                wrongNoteRecentQueuePort,
                rulebookFileStore,
                rulebookIndexPort,
                objectMapper
        );
    }
}
