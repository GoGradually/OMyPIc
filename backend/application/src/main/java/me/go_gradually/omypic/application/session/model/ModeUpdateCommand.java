package me.go_gradually.omypic.application.session.model;

import me.go_gradually.omypic.domain.session.ModeType;

import java.util.ArrayList;
import java.util.List;

public class ModeUpdateCommand {
    private String sessionId;
    private ModeType mode;
    private Integer continuousBatchSize;
    private List<String> selectedGroupTags = new ArrayList<>();

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
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

    public List<String> getSelectedGroupTags() {
        return selectedGroupTags;
    }

    public void setSelectedGroupTags(List<String> selectedGroupTags) {
        this.selectedGroupTags = selectedGroupTags == null ? new ArrayList<>() : new ArrayList<>(selectedGroupTags);
    }
}
