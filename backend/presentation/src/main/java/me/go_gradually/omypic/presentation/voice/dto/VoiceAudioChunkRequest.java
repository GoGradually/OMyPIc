package me.go_gradually.omypic.presentation.voice.dto;

import jakarta.validation.constraints.NotBlank;

public class VoiceAudioChunkRequest {
    @NotBlank
    private String pcm16Base64;
    private Integer sampleRate;
    private Long sequence;

    public String getPcm16Base64() {
        return pcm16Base64;
    }

    public void setPcm16Base64(String pcm16Base64) {
        this.pcm16Base64 = pcm16Base64;
    }

    public Integer getSampleRate() {
        return sampleRate;
    }

    public void setSampleRate(Integer sampleRate) {
        this.sampleRate = sampleRate;
    }

    public Long getSequence() {
        return sequence;
    }

    public void setSequence(Long sequence) {
        this.sequence = sequence;
    }
}
