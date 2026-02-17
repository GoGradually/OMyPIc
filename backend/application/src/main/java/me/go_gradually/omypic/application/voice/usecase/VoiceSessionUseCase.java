package me.go_gradually.omypic.application.voice.usecase;

import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.application.shared.port.AsyncExecutor;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.application.stt.usecase.SttUseCase;
import me.go_gradually.omypic.application.voice.model.VoiceAudioChunkCommand;
import me.go_gradually.omypic.application.voice.model.VoiceEventSink;
import me.go_gradually.omypic.application.voice.model.VoiceSessionOpenCommand;
import me.go_gradually.omypic.application.voice.model.VoiceSessionStopCommand;
import me.go_gradually.omypic.application.voice.policy.VoicePolicy;
import me.go_gradually.omypic.application.voice.port.TtsGateway;

import java.util.function.LongSupplier;

public class VoiceSessionUseCase {
    private final VoiceSessionFacade facade;

    public VoiceSessionUseCase(SttUseCase sttUseCase,
                               FeedbackUseCase feedbackUseCase,
                               SessionUseCase sessionUseCase,
                               QuestionUseCase questionUseCase,
                               TtsGateway ttsGateway,
                               AsyncExecutor asyncExecutor,
                               VoicePolicy voicePolicy,
                               MetricsPort metrics) {
        this.facade = new VoiceSessionFacade(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics
        );
    }

    VoiceSessionUseCase(SttUseCase sttUseCase,
                        FeedbackUseCase feedbackUseCase,
                        SessionUseCase sessionUseCase,
                        QuestionUseCase questionUseCase,
                        TtsGateway ttsGateway,
                        AsyncExecutor asyncExecutor,
                        VoicePolicy voicePolicy,
                        MetricsPort metrics,
                        int eventReplayBufferLimit) {
        this.facade = new VoiceSessionFacade(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics,
                eventReplayBufferLimit
        );
    }

    VoiceSessionUseCase(SttUseCase sttUseCase,
                        FeedbackUseCase feedbackUseCase,
                        SessionUseCase sessionUseCase,
                        QuestionUseCase questionUseCase,
                        TtsGateway ttsGateway,
                        AsyncExecutor asyncExecutor,
                        VoicePolicy voicePolicy,
                        MetricsPort metrics,
                        int eventReplayBufferLimit,
                        LongSupplier nowMillisSupplier) {
        this.facade = new VoiceSessionFacade(
                sttUseCase,
                feedbackUseCase,
                sessionUseCase,
                questionUseCase,
                ttsGateway,
                asyncExecutor,
                voicePolicy,
                metrics,
                eventReplayBufferLimit,
                nowMillisSupplier
        );
    }

    public String open(VoiceSessionOpenCommand command) {
        return facade.open(command);
    }

    public void registerSink(String voiceSessionId, VoiceEventSink sink) {
        facade.registerSink(voiceSessionId, sink);
    }

    public void registerSink(String voiceSessionId, VoiceEventSink sink, Long sinceEventId) {
        facade.registerSink(voiceSessionId, sink, sinceEventId);
    }

    public void unregisterSink(String voiceSessionId, VoiceEventSink sink) {
        facade.unregisterSink(voiceSessionId, sink);
    }

    public RecoverySnapshot recover(String voiceSessionId, Long lastSeenEventId) {
        return toRecoverySnapshot(facade.recover(voiceSessionId, lastSeenEventId));
    }

    public void appendAudio(VoiceAudioChunkCommand command) {
        facade.appendAudio(command);
    }

    public void stop(VoiceSessionStopCommand command) {
        facade.stop(command);
    }

    private RecoverySnapshot toRecoverySnapshot(VoiceSessionFacade.RecoverySnapshot snapshot) {
        return new RecoverySnapshot(
                snapshot.sessionId(),
                snapshot.voiceSessionId(),
                snapshot.active(),
                snapshot.stopped(),
                snapshot.stopReason(),
                snapshot.currentTurnId(),
                toRecoveryQuestion(snapshot.currentQuestion()),
                snapshot.turnProcessing(),
                snapshot.hasBufferedAudio(),
                snapshot.lastAcceptedChunkSequence(),
                snapshot.latestEventId(),
                snapshot.replayFromEventId(),
                snapshot.gapDetected()
        );
    }

    private RecoveryQuestion toRecoveryQuestion(VoiceSessionFacade.RecoveryQuestion question) {
        if (question == null) {
            return null;
        }
        return new RecoveryQuestion(
                question.id(),
                question.text(),
                question.group(),
                question.groupId(),
                question.questionType()
        );
    }

    public record RecoveryQuestion(String id,
                                   String text,
                                   String group,
                                   String groupId,
                                   String questionType) {
    }

    public record RecoverySnapshot(String sessionId,
                                   String voiceSessionId,
                                   boolean active,
                                   boolean stopped,
                                   String stopReason,
                                   long currentTurnId,
                                   RecoveryQuestion currentQuestion,
                                   boolean turnProcessing,
                                   boolean hasBufferedAudio,
                                   Long lastAcceptedChunkSequence,
                                   long latestEventId,
                                   long replayFromEventId,
                                   boolean gapDetected) {
    }
}
