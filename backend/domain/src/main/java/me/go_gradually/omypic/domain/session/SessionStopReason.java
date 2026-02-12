package me.go_gradually.omypic.domain.session;

public enum SessionStopReason {
    QUESTION_EXHAUSTED("QUESTION_EXHAUSTED");

    private final String code;

    SessionStopReason(String code) {
        this.code = code;
    }

    public String code() {
        return code;
    }
}
