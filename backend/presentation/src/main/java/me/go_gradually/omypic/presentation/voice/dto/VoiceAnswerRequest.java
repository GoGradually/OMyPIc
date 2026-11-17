package me.go_gradually.omypic.presentation.voice.dto;

public class VoiceAnswerRequest {
    private Long sequence;
    private String text;

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
