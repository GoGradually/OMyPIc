package me.go_gradually.omypic.presentation.feedback.dto;

import java.util.ArrayList;
import java.util.List;

public class FeedbackResponse {
    private String summary;
    private List<String> correctionPoints = new ArrayList<>();
    private String exampleAnswer;
    private List<String> rulebookEvidence = new ArrayList<>();

    public String getSummary() {
        return summary;
    }

    public void setSummary(String summary) {
        this.summary = summary;
    }

    public List<String> getCorrectionPoints() {
        return correctionPoints;
    }

    public void setCorrectionPoints(List<String> correctionPoints) {
        this.correctionPoints = correctionPoints;
    }

    public String getExampleAnswer() {
        return exampleAnswer;
    }

    public void setExampleAnswer(String exampleAnswer) {
        this.exampleAnswer = exampleAnswer;
    }

    public List<String> getRulebookEvidence() {
        return rulebookEvidence;
    }

    public void setRulebookEvidence(List<String> rulebookEvidence) {
        this.rulebookEvidence = rulebookEvidence;
    }
}
