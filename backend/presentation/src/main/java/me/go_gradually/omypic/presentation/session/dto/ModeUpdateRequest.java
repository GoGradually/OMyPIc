package me.go_gradually.omypic.presentation.session.dto;

import me.go_gradually.omypic.domain.session.ModeType;

public class ModeUpdateRequest {
    private String sessionId;
    private String listId;
    private ModeType mode;
    private Integer continuousBatchSize;

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public String getListId() {
        return listId;
    }

    public void setListId(String listId) {
        this.listId = listId;
    }

    public ModeType getMode() {
        return mode;
    }

    public void setMode(ModeType mode) {
        this.mode = mode;
    }

    public Integer getContinuousBatchSize() {
        return continuousBatchSize;
    }

    public void setContinuousBatchSize(Integer continuousBatchSize) {
        this.continuousBatchSize = continuousBatchSize;
    }
}
