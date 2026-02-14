package me.go_gradually.omypic.presentation.voice.dto;

public class VoiceSessionStopRequest {
    private Boolean forced;
    private String reason;

    public Boolean getForced() {
        return forced;
    }

    public void setForced(Boolean forced) {
        this.forced = forced;
    }

    public String getReason() {
        return reason;
    }

    public void setReason(String reason) {
        this.reason = reason;
    }
}
