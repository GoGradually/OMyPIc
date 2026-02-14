package me.go_gradually.omypic.domain.feedback;

public record Corrections(CorrectionDetail grammar, CorrectionDetail expression, CorrectionDetail logic) {
    public Corrections {
        grammar = grammar == null ? new CorrectionDetail("", "") : grammar;
        expression = expression == null ? new CorrectionDetail("", "") : expression;
        logic = logic == null ? new CorrectionDetail("", "") : logic;
    }
}
