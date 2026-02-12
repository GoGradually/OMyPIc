package me.go_gradually.omypic.domain.session;

public final class SessionFlowPolicy {
    private SessionFlowPolicy() {
    }

    public static SessionAction decideAfterQuestionSelection(boolean exhausted) {
        if (exhausted) {
            return SessionAction.autoStop(SessionStopReason.QUESTION_EXHAUSTED);
        }
        return SessionAction.askNext();
    }

    public record SessionAction(NextActionType type, SessionStopReason reason) {
        public static SessionAction askNext() {
            return new SessionAction(NextActionType.ASK_NEXT, null);
        }

        public static SessionAction autoStop(SessionStopReason reason) {
            if (reason == null) {
                throw new IllegalArgumentException("Session stop reason is required");
            }
            return new SessionAction(NextActionType.AUTO_STOP, reason);
        }
    }

    public enum NextActionType {
        ASK_NEXT,
        AUTO_STOP
    }
}
