package me.go_gradually.omypic.application.voice.model;

public class VoiceSessionStopCommand {
    private String voiceSessionId;
    private boolean forced = true;
    private String reason;

    public String getVoiceSessionId() {
        return voiceSessionId;
    }

    public void setVoiceSessionId(String voiceSessionId) {
        this.voiceSessionId = voiceSessionId;
    }

    public boolean isForced() {
        return forced;
    }

    public void setForced(boolean forced) {
        this.forced = forced;
    }

    public String getReason() {
        return reason;
    }

    public void setReason(String reason) {
        this.reason = reason;
    }
}
