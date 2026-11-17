package me.go_gradually.omypic.application.voice.model;

/**
 * Signals that the learner finished answering the current question in a session that runs
 * without AI. The recording stays on the client; only the turn boundary reaches the server.
 */
public class VoiceAnswerCommand {
    private String voiceSessionId;
    private Long sequence;

    public String getVoiceSessionId() {
        return voiceSessionId;
    }

    public void setVoiceSessionId(String voiceSessionId) {
        this.voiceSessionId = voiceSessionId;
    }

    public Long getSequence() {
        return sequence;
    }

    public void setSequence(Long sequence) {
        this.sequence = sequence;
    }
}
