package me.go_gradually.omypic.presentation.voice.dto;

public class VoiceSessionRecoveryResponse {
    private String sessionId;
    private String voiceSessionId;
    private boolean active;
    private boolean stopped;
    private String stopReason;
    private long currentTurnId;
    private RecoveryQuestion currentQuestion;
    private boolean turnProcessing;
    private boolean hasBufferedAudio;
    private Long lastAcceptedChunkSequence;
    private long latestEventId;
    private long replayFromEventId;
    private boolean gapDetected;

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public String getVoiceSessionId() {
        return voiceSessionId;
    }

    public void setVoiceSessionId(String voiceSessionId) {
        this.voiceSessionId = voiceSessionId;
    }

    public boolean isActive() {
        return active;
    }

    public void setActive(boolean active) {
        this.active = active;
    }

    public boolean isStopped() {
        return stopped;
    }

    public void setStopped(boolean stopped) {
        this.stopped = stopped;
    }

    public String getStopReason() {
        return stopReason;
    }

    public void setStopReason(String stopReason) {
        this.stopReason = stopReason;
    }

    public long getCurrentTurnId() {
        return currentTurnId;
    }

    public void setCurrentTurnId(long currentTurnId) {
        this.currentTurnId = currentTurnId;
    }

    public RecoveryQuestion getCurrentQuestion() {
        return currentQuestion;
    }

    public void setCurrentQuestion(RecoveryQuestion currentQuestion) {
        this.currentQuestion = currentQuestion;
    }

    public boolean isTurnProcessing() {
        return turnProcessing;
    }

    public void setTurnProcessing(boolean turnProcessing) {
        this.turnProcessing = turnProcessing;
    }

    public boolean isHasBufferedAudio() {
        return hasBufferedAudio;
    }

    public void setHasBufferedAudio(boolean hasBufferedAudio) {
        this.hasBufferedAudio = hasBufferedAudio;
    }

    public Long getLastAcceptedChunkSequence() {
        return lastAcceptedChunkSequence;
    }

    public void setLastAcceptedChunkSequence(Long lastAcceptedChunkSequence) {
        this.lastAcceptedChunkSequence = lastAcceptedChunkSequence;
    }

    public long getLatestEventId() {
        return latestEventId;
    }

    public void setLatestEventId(long latestEventId) {
        this.latestEventId = latestEventId;
    }

    public long getReplayFromEventId() {
        return replayFromEventId;
    }

    public void setReplayFromEventId(long replayFromEventId) {
        this.replayFromEventId = replayFromEventId;
    }

    public boolean isGapDetected() {
        return gapDetected;
    }

    public void setGapDetected(boolean gapDetected) {
        this.gapDetected = gapDetected;
    }

    public static class RecoveryQuestion {
        private String id;
        private String text;
        private String group;
        private String groupId;
        private String questionType;

        public String getId() {
            return id;
        }

        public void setId(String id) {
            this.id = id;
        }

        public String getText() {
            return text;
        }

        public void setText(String text) {
            this.text = text;
        }

        public String getGroup() {
            return group;
        }

        public void setGroup(String group) {
            this.group = group;
        }

        public String getGroupId() {
            return groupId;
        }

        public void setGroupId(String groupId) {
            this.groupId = groupId;
        }

        public String getQuestionType() {
            return questionType;
        }

        public void setQuestionType(String questionType) {
            this.questionType = questionType;
        }
    }
}
