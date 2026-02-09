package me.go_gradually.omypic.presentation.tts.dto;

import jakarta.validation.constraints.NotBlank;

public class TtsRequest {
    @NotBlank
    private String text;
    @NotBlank
    private String voice;

    public String getText() {
        return text;
    }

    public void setText(String text) {
        this.text = text;
    }

    public String getVoice() {
        return voice;
    }

    public void setVoice(String voice) {
        this.voice = voice;
    }
}
