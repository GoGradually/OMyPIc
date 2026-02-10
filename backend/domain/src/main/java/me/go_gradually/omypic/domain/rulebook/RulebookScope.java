package me.go_gradually.omypic.domain.rulebook;

public enum RulebookScope {
    MAIN,
    QUESTION;

    public static RulebookScope from(String value) {
        if (value == null || value.isBlank()) {
            return MAIN;
        }
        String normalized = value.trim().toUpperCase();
        if ("QUESTION".equals(normalized)) {
            return QUESTION;
        }
        return MAIN;
    }
}
