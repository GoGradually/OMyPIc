package me.go_gradually.omypic.domain.session;

import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionItem;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Deque;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

public class SessionState {
    private final SessionId sessionId;
    private final Deque<String> sttSegments = new ArrayDeque<>();
    private final Set<String> selectedGroupTags = new LinkedHashSet<>();
    private final List<String> candidateGroupOrder = new ArrayList<>();
    private final Map<String, Integer> groupQuestionIndices = new ConcurrentHashMap<>();

    private ModeType mode = ModeType.IMMEDIATE;
    private int continuousBatchSize = 3;
    private int completedGroupCountSinceLastFeedback = 0;
    private int currentGroupCursor = 0;
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

    public int getCompletedGroupCountSinceLastFeedback() {
        return completedGroupCountSinceLastFeedback;
    }

    public Set<String> getSelectedGroupTags() {
        return Collections.unmodifiableSet(selectedGroupTags);
    }

    public List<String> getCandidateGroupOrder() {
        return List.copyOf(candidateGroupOrder);
    }

    public Map<String, Integer> getGroupQuestionIndices() {
        return Collections.unmodifiableMap(groupQuestionIndices);
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
            this.completedGroupCountSinceLastFeedback = 0;
        }
    }

    public void configureQuestionGroups(Set<String> selectedTags, List<String> candidateGroupIds) {
        resetSelectedTags(selectedTags);
        resetCandidateGroupOrder(candidateGroupIds);
        resetQuestionProgress();
    }

    private void resetSelectedTags(Set<String> selectedTags) {
        selectedGroupTags.clear();
        if (selectedTags != null) {
            selectedGroupTags.addAll(selectedTags);
        }
    }

    private void resetCandidateGroupOrder(List<String> candidateGroupIds) {
        candidateGroupOrder.clear();
        if (candidateGroupIds == null) {
            return;
        }
        for (String groupId : candidateGroupIds) {
            if (groupId != null && !groupId.isBlank()) {
                candidateGroupOrder.add(groupId);
            }
        }
    }

    private void resetQuestionProgress() {
        groupQuestionIndices.clear();
        currentGroupCursor = 0;
        completedGroupCountSinceLastFeedback = 0;
    }

    public TurnBatchingPolicy.BatchDecision decideFeedbackBatchOnTurn(boolean completedGroupThisTurn) {
        TurnBatchingPolicy.BatchDecision decision = TurnBatchingPolicy.onTurn(
                mode,
                completedGroupCountSinceLastFeedback,
                continuousBatchSize,
                completedGroupThisTurn
        );
        completedGroupCountSinceLastFeedback = decision.nextCompletedGroupCount();
        return decision;
    }

    public boolean shouldGenerateFeedback() {
        return decideFeedbackBatchOnTurn(true).emitFeedback();
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
        return String.join("\n", list);
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

    public String currentCandidateGroupId() {
        if (currentGroupCursor < 0 || currentGroupCursor >= candidateGroupOrder.size()) {
            return null;
        }
        return candidateGroupOrder.get(currentGroupCursor);
    }

    public void moveToNextGroup() {
        currentGroupCursor = Math.min(candidateGroupOrder.size(), currentGroupCursor + 1);
    }

    public int getCurrentQuestionIndex(String groupId) {
        if (groupId == null || groupId.isBlank()) {
            return 0;
        }
        return Math.max(0, groupQuestionIndices.getOrDefault(groupId, 0));
    }

    public void markQuestionAsked(String groupId) {
        if (groupId == null || groupId.isBlank()) {
            return;
        }
        groupQuestionIndices.merge(groupId, 1, Integer::sum);
    }

    public Optional<QuestionItem> nextQuestion(QuestionGroupAggregate group) {
        if (group == null) {
            return Optional.empty();
        }
        int index = getCurrentQuestionIndex(group.getId().value());
        if (index >= group.getQuestions().size()) {
            return Optional.empty();
        }
        QuestionItem item = group.getQuestions().get(index);
        markQuestionAsked(group.getId().value());
        return Optional.of(item);
    }
}
