package me.go_gradually.omypic.domain.feedback;

public record CorrectionDetail(String issue, String fix) {
    public CorrectionDetail {
        issue = issue == null ? "" : issue.trim();
        fix = fix == null ? "" : fix.trim();
    }
}
