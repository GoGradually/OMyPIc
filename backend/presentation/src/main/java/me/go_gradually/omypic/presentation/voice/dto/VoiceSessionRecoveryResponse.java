package me.go_gradually.omypic.presentation.voice.dto;

public record VoiceSessionRecoveryResponse(String sessionId,
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
    public record RecoveryQuestion(String id,
                                   String text,
                                   String group,
                                   String groupId,
                                   String questionType) {
    }
}
