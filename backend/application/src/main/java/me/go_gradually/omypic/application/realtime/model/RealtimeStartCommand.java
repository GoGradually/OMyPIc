package me.go_gradually.omypic.application.realtime.model;

public class RealtimeStartCommand {
    private String sessionId;
    private String apiKey;
    private String conversationModel;
    private String sttModel;

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

    public String getConversationModel() {
        return conversationModel;
    }

    public void setConversationModel(String conversationModel) {
        this.conversationModel = conversationModel;
    }

    public String getSttModel() {
        return sttModel;
    }

    public void setSttModel(String sttModel) {
        this.sttModel = sttModel;
    }
}
