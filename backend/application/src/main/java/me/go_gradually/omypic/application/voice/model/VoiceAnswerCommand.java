package me.go_gradually.omypic.application.voice.model;

/**
 * Reports that the learner finished answering the current question without uploading audio.
 * With AI off only the turn boundary is sent. With on-device speech recognition the client
 * also sends the transcript it produced.
 */
public class VoiceAnswerCommand {
    private String voiceSessionId;
    private Long sequence;
    private String text;

    public String getVoiceSessionId() {
        return voiceSessionId;
    }

    public void setVoiceSessionId(String voiceSessionId) {
        this.voiceSessionId = voiceSessionId;
    }

    public String getText() {
        return text;
    }

    public void setText(String text) {
        this.text = text;
    }

    public Long getSequence() {
        return sequence;
    }

    public void setSequence(Long sequence) {
        this.sequence = sequence;
    }
}
