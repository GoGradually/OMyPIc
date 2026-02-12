package me.go_gradually.omypic.domain.session;

import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Deque;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

public class SessionState {
    private final SessionId sessionId;
    private final Deque<String> sttSegments = new ArrayDeque<>();
    private final Map<QuestionListId, Integer> listIndices = new ConcurrentHashMap<>();
    private QuestionListId activeQuestionListId;
    private ModeType mode = ModeType.IMMEDIATE;
    private int continuousBatchSize = 3;
    private int answeredSinceLastFeedback = 0;
    private FeedbackLanguage feedbackLanguage = FeedbackLanguage.of("ko");

    public SessionState(SessionId sessionId) {
        if (sessionId == null) {
            throw new IllegalArgumentException("SessionId is required");
        }
        this.sessionId = sessionId;
    }

    public SessionId getSessionId() {
        return sessionId;
    }

    public ModeType getMode() {
        return mode;
    }

    public int getContinuousBatchSize() {
        return continuousBatchSize;
    }

    public int getAnsweredSinceLastFeedback() {
        return answeredSinceLastFeedback;
    }

    public String getActiveQuestionListId() {
        return activeQuestionListId == null ? null : activeQuestionListId.value();
    }

    public void setActiveQuestionListId(String listId) {
        if (listId == null || listId.isBlank()) {
            this.activeQuestionListId = null;
            return;
        }
        this.activeQuestionListId = QuestionListId.of(listId);
    }

    public void applyModeUpdate(ModeType mode, Integer continuousBatchSize) {
        ModeType previous = this.mode;
        if (mode != null) {
            this.mode = mode;
        }
        if (continuousBatchSize != null) {
            this.continuousBatchSize = Math.min(10, Math.max(1, continuousBatchSize));
        }
        if (previous != this.mode) {
            this.answeredSinceLastFeedback = 0;
        }
    }

    public TurnBatchingPolicy.BatchDecision decideFeedbackBatchOnAnswer() {
        TurnBatchingPolicy.BatchDecision decision =
                TurnBatchingPolicy.onAnsweredTurn(mode, answeredSinceLastFeedback, continuousBatchSize);
        answeredSinceLastFeedback = decision.nextAnsweredCount();
        return decision;
    }

    public boolean shouldGenerateFeedback() {
        return decideFeedbackBatchOnAnswer().emitFeedback();
    }

    public boolean shouldGenerateResidualContinuousFeedback(boolean questionExhausted,
                                                            boolean emittedFeedbackThisTurn) {
        return TurnBatchingPolicy.shouldEmitResidualContinuousBatch(mode, questionExhausted, emittedFeedbackThisTurn);
    }

    public String resolveFeedbackInputText(String fallbackText) {
        if (mode != ModeType.CONTINUOUS) {
            return fallbackText == null ? "" : fallbackText;
        }
        List<String> list = new ArrayList<>(sttSegments);
        if (list.isEmpty()) {
            return fallbackText == null ? "" : fallbackText;
        }
        int from = Math.max(0, list.size() - continuousBatchSize);
        return String.join("\n", list.subList(from, list.size()));
    }

    public List<String> getSttSegments() {
        return List.copyOf(sttSegments);
    }

    public void appendSegment(String text) {
        if (text == null || text.isBlank()) {
            return;
        }
        sttSegments.add(text);
    }

    public FeedbackLanguage getFeedbackLanguage() {
        return feedbackLanguage;
    }

    public void setFeedbackLanguage(FeedbackLanguage feedbackLanguage) {
        this.feedbackLanguage = feedbackLanguage == null ? FeedbackLanguage.of("ko") : feedbackLanguage;
    }

    public Optional<QuestionItem> nextQuestion(QuestionList list) {
        if (list == null) {
            return Optional.empty();
        }
        return nextSequential(list);
    }

    public void resetQuestionProgress(String listId) {
        if (listId == null || listId.isBlank()) {
            return;
        }
        listIndices.put(QuestionListId.of(listId), 0);
        answeredSinceLastFeedback = 0;
    }

    private Optional<QuestionItem> nextSequential(QuestionList list) {
        List<QuestionItem> questions = list.getQuestions();
        if (questions.isEmpty()) {
            return Optional.empty();
        }
        int index = listIndices.getOrDefault(list.getId(), 0);
        if (index >= questions.size()) {
            return Optional.empty();
        }
        QuestionItem item = questions.get(index);
        listIndices.put(list.getId(), index + 1);
        return Optional.ofNullable(item);
    }

    public Map<QuestionListId, Integer> getListIndices() {
        return Collections.unmodifiableMap(listIndices);
    }
}
