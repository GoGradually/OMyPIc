package me.go_gradually.omypic.application.feedback.policy;

public interface FeedbackPolicy {
    int getSummaryMaxChars();

    double getExampleMinRatio();

    double getExampleMaxRatio();

    int getWrongnoteSummaryMaxChars();

    default int getWrongnoteWindowSize() {
        return 30;
    }
}
