package me.go_gradually.omypic.bootstrap;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.question.port.QuestionListPort;
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
import me.go_gradually.omypic.application.stt.port.SttJobStorePort;
import me.go_gradually.omypic.application.stt.usecase.SttJobUseCase;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.application.tts.port.TtsGateway;
import me.go_gradually.omypic.application.tts.usecase.TtsUseCase;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
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
    public QuestionUseCase questionUseCase(QuestionListPort questionListPort,
                                           SessionStorePort sessionStore,
                                           MetricsPort metricsPort) {
        return new QuestionUseCase(questionListPort, sessionStore, metricsPort);
    }

    @Bean
    public SessionUseCase sessionUseCase(SessionStorePort sessionStore,
                                         QuestionListPort questionListPort) {
        return new SessionUseCase(sessionStore, questionListPort);
    }

    @Bean
    public RulebookUseCase rulebookUseCase(RulebookPort rulebookPort,
                                           RulebookIndexPort rulebookIndexPort,
                                           RulebookFileStore rulebookFileStore,
                                           RagPolicy ragPolicy) {
        return new RulebookUseCase(rulebookPort, rulebookIndexPort, rulebookFileStore, ragPolicy);
    }

    @Bean
    public WrongNoteUseCase wrongNoteUseCase(WrongNotePort wrongNotePort,
                                             FeedbackPolicy feedbackPolicy) {
        return new WrongNoteUseCase(wrongNotePort, feedbackPolicy);
    }

    @Bean
    public FeedbackUseCase feedbackUseCase(List<LlmClient> llmClients,
                                           RulebookUseCase rulebookUseCase,
                                           FeedbackPolicy feedbackPolicy,
                                           MetricsPort metricsPort,
                                           SessionStorePort sessionStore,
                                           WrongNoteUseCase wrongNoteUseCase) {
        return new FeedbackUseCase(llmClients, rulebookUseCase, feedbackPolicy, metricsPort, sessionStore, wrongNoteUseCase);
    }

    @Bean
    public SttUseCase sttUseCase(SttGateway sttGateway,
                                 SttPolicy sttPolicy,
                                 MetricsPort metricsPort) {
        return new SttUseCase(sttGateway, sttPolicy, metricsPort);
    }

    @Bean
    public SttJobUseCase sttJobUseCase(SttUseCase sttUseCase,
                                       SessionStorePort sessionStore,
                                       SttJobStorePort jobStore,
                                       AsyncExecutor asyncExecutor) {
        return new SttJobUseCase(sttUseCase, sessionStore, jobStore, asyncExecutor);
    }

    @Bean
    public TtsUseCase ttsUseCase(TtsGateway ttsGateway,
                                 MetricsPort metricsPort) {
        return new TtsUseCase(ttsGateway, metricsPort);
    }
}
