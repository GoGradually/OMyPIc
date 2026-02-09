package me.go_gradually.omypic.application.realtime.model;

public class RealtimeStartCommand {
    private String sessionId;
    private String apiKey;

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public String getApiKey() {
        return apiKey;
    }

    public void setApiKey(String apiKey) {
        this.apiKey = apiKey;
    }
}
