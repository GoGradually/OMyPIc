package me.go_gradually.omypic.domain.session;

import me.go_gradually.omypic.domain.feedback.FeedbackLanguage;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;

import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import java.util.stream.Collectors;

public class SessionState {
    private final SessionId sessionId;
    private final Deque<String> sttSegments = new ArrayDeque<>();
    private final Map<QuestionListId, Integer> listIndices = new ConcurrentHashMap<>();
    private QuestionListId activeQuestionListId;
    private ModeType mode = ModeType.IMMEDIATE;
    private int continuousBatchSize = 3;
    private int answeredSinceLastFeedback = 0;
    private FeedbackLanguage feedbackLanguage = FeedbackLanguage.of("ko");
    private MockExamState mockExamState;
    private boolean mockExamCompleted = false;
    private boolean mockFinalFeedbackGenerated = false;
    private int mockExamStartSegmentIndex = 0;

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
            if (this.mode != ModeType.MOCK_EXAM) {
                this.mockExamCompleted = false;
                this.mockFinalFeedbackGenerated = false;
                this.mockExamStartSegmentIndex = sttSegments.size();
            }
        }
    }

    public boolean shouldGenerateFeedback() {
        //TODO 모의고사의 경우, 모든 질문을 완료해야 피드백을 받을 수 있다.
        if (mode == ModeType.MOCK_EXAM) {
            return false;
        }
        if (mode == ModeType.CONTINUOUS) {
            answeredSinceLastFeedback += 1;
            if (answeredSinceLastFeedback < continuousBatchSize) {
                return false;
            }
            answeredSinceLastFeedback = 0;
        }
        return true;
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

    public MockExamState getMockExamState() {
        return mockExamState;
    }

    public boolean isMockExamCompleted() {
        return mockExamCompleted;
    }

    public boolean isMockFinalFeedbackGenerated() {
        return mockFinalFeedbackGenerated;
    }

    public void configureMockExam(QuestionList list,
                                  List<String> groupOrder,
                                  Map<String, Integer> groupCounts) {
        if (list == null) {
            this.mockExamState = null;
            this.mockExamCompleted = false;
            this.mockFinalFeedbackGenerated = false;
            this.mockExamStartSegmentIndex = sttSegments.size();
            return;
        }
        List<String> order = groupOrder == null ? List.of() : groupOrder;
        Map<String, Integer> counts = groupCounts == null ? Map.of() : groupCounts;
        Map<String, List<QuestionItemId>> grouped = list.getQuestions().stream()
                .filter(q -> q.getGroup() != null)
                .collect(Collectors.groupingBy(QuestionItem::getGroup,
                        Collectors.mapping(QuestionItem::getId, Collectors.toList())));
        Map<String, List<QuestionItemId>> remaining = new HashMap<>();
        for (Map.Entry<String, List<QuestionItemId>> entry : grouped.entrySet()) {
            List<QuestionItemId> shuffled = new ArrayList<>(entry.getValue());
            Collections.shuffle(shuffled);
            remaining.put(entry.getKey(), shuffled);
        }
        this.mockExamState = new MockExamState(order, counts, remaining);
        this.mockExamCompleted = false;
        this.mockFinalFeedbackGenerated = false;
        this.mockExamStartSegmentIndex = sttSegments.size();
    }

    public Optional<QuestionItem> nextQuestion(QuestionList list) {
        if (list == null) {
            return Optional.empty();
        }
        if (mode == ModeType.MOCK_EXAM) {
            Optional<QuestionItem> next = nextMockExam(list);
            mockExamCompleted = next.isEmpty();
            return next;
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

    public String buildMockFinalFeedbackInput() {
        if (!mockExamCompleted) {
            throw new IllegalStateException("Mock exam is not completed");
        }
        List<String> all = new ArrayList<>(sttSegments);
        int from = Math.max(0, Math.min(mockExamStartSegmentIndex, all.size()));
        List<String> mockAnswers = all.subList(from, all.size());
        if (mockAnswers.isEmpty()) {
            throw new IllegalStateException("No answers available for mock final feedback");
        }
        return String.join("\n", mockAnswers);
    }

    public void markMockFinalFeedbackGenerated() {
        if (!mockExamCompleted) {
            throw new IllegalStateException("Mock exam is not completed");
        }
        if (mockFinalFeedbackGenerated) {
            throw new IllegalStateException("Mock final feedback already generated");
        }
        mockFinalFeedbackGenerated = true;
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

    private Optional<QuestionItem> nextMockExam(QuestionList list) {
        MockExamState mock = mockExamState;
        if (mock == null || mock.groupOrder.isEmpty()) {
            return nextSequential(list);
        }
        while (mock.groupIndex < mock.groupOrder.size()) {
            String group = mock.groupOrder.get(mock.groupIndex);
            int target = mock.groupCounts.getOrDefault(group, 0);
            int selected = mock.selectedCounts.getOrDefault(group, 0);
            if (selected >= target) {
                mock.groupIndex += 1;
                continue;
            }
            List<QuestionItemId> remaining = mock.remainingQuestions.get(group);
            if (remaining == null || remaining.isEmpty()) {
                mock.groupIndex += 1;
                continue;
            }
            QuestionItemId questionId = remaining.remove(0);
            mock.selectedCounts.put(group, selected + 1);
            QuestionItem item = list.getQuestions().stream()
                    .filter(q -> q.getId().equals(questionId))
                    .findFirst()
                    .orElse(null);
            if (item == null) {
                continue;
            }
            return Optional.of(item);
        }
        return Optional.empty();
    }

    public Map<QuestionListId, Integer> getListIndices() {
        return Collections.unmodifiableMap(listIndices);
    }

    public static class MockExamState {
        private final List<String> groupOrder = new ArrayList<>();
        private final Map<String, Integer> groupCounts;
        private final Map<String, List<QuestionItemId>> remainingQuestions;
        private final Map<String, Integer> selectedCounts = new ConcurrentHashMap<>();
        private int groupIndex = 0;

        private MockExamState(List<String> groupOrder,
                              Map<String, Integer> groupCounts,
                              Map<String, List<QuestionItemId>> remainingQuestions) {
            if (groupOrder != null) {
                this.groupOrder.addAll(groupOrder);
            }
            this.groupCounts = new HashMap<>(groupCounts == null ? Map.of() : groupCounts);
            this.remainingQuestions = new HashMap<>(remainingQuestions == null ? Map.of() : remainingQuestions);
        }

        public List<String> getGroupOrder() {
            return Collections.unmodifiableList(groupOrder);
        }

        public Map<String, Integer> getGroupCounts() {
            return Collections.unmodifiableMap(groupCounts);
        }

        public Map<String, List<QuestionItemId>> getRemainingQuestions() {
            Map<String, List<QuestionItemId>> copy = new HashMap<>();
            for (Map.Entry<String, List<QuestionItemId>> entry : remainingQuestions.entrySet()) {
                copy.put(entry.getKey(), Collections.unmodifiableList(entry.getValue()));
            }
            return Collections.unmodifiableMap(copy);
        }

        public Map<String, Integer> getSelectedCounts() {
            return Collections.unmodifiableMap(selectedCounts);
        }

        public int getGroupIndex() {
            return groupIndex;
        }
    }
}
