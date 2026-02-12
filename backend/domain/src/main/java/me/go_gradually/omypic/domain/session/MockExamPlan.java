package me.go_gradually.omypic.domain.session;

import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.stream.Collectors;

public final class MockExamPlan {
    private final List<QuestionGroup> groupOrder = new ArrayList<>();
    private final Map<QuestionGroup, Integer> groupCounts;
    private final Map<QuestionGroup, List<QuestionItemId>> remainingQuestions;
    private final Map<QuestionGroup, Integer> selectedCounts = new ConcurrentHashMap<>();
    private int groupIndex;

    private MockExamPlan(List<QuestionGroup> groupOrder,
                         Map<QuestionGroup, Integer> groupCounts,
                         Map<QuestionGroup, List<QuestionItemId>> remainingQuestions) {
        if (groupOrder != null) {
            this.groupOrder.addAll(groupOrder);
        }
        this.groupCounts = new LinkedHashMap<>(groupCounts == null ? Map.of() : groupCounts);
        this.remainingQuestions = new HashMap<>(remainingQuestions == null ? Map.of() : remainingQuestions);
    }

    public static MockExamPlan fromQuestionList(QuestionList list,
                                                List<QuestionGroup> groupOrder,
                                                Map<QuestionGroup, Integer> groupCounts) {
        if (list == null) {
            return new MockExamPlan(List.of(), Map.of(), Map.of());
        }
        List<QuestionGroup> resolvedOrder = groupOrder == null ? List.of() : groupOrder;
        Map<QuestionGroup, Integer> resolvedCounts = groupCounts == null ? Map.of() : groupCounts;
        Map<QuestionGroup, List<QuestionItemId>> grouped = list.getQuestions().stream()
                .filter(question -> question.getGroup() != null)
                .collect(Collectors.groupingBy(QuestionItem::getGroup,
                        Collectors.mapping(QuestionItem::getId, Collectors.toList())));
        Map<QuestionGroup, List<QuestionItemId>> remaining = new HashMap<>();
        for (Map.Entry<QuestionGroup, List<QuestionItemId>> entry : grouped.entrySet()) {
            List<QuestionItemId> shuffled = new ArrayList<>(entry.getValue());
            Collections.shuffle(shuffled);
            remaining.put(entry.getKey(), shuffled);
        }
        return new MockExamPlan(resolvedOrder, resolvedCounts, remaining);
    }

    public boolean hasConfiguredOrder() {
        return !groupOrder.isEmpty();
    }

    public Optional<QuestionItemId> nextQuestionId() {
        while (groupIndex < groupOrder.size()) {
            QuestionGroup group = groupOrder.get(groupIndex);
            int target = Math.max(0, groupCounts.getOrDefault(group, 0));
            int selected = Math.max(0, selectedCounts.getOrDefault(group, 0));
            if (selected >= target) {
                groupIndex += 1;
                continue;
            }
            List<QuestionItemId> remaining = remainingQuestions.get(group);
            if (remaining == null || remaining.isEmpty()) {
                groupIndex += 1;
                continue;
            }
            QuestionItemId questionId = remaining.remove(0);
            selectedCounts.put(group, selected + 1);
            return Optional.of(questionId);
        }
        return Optional.empty();
    }

    public List<QuestionGroup> getGroupOrder() {
        return Collections.unmodifiableList(groupOrder);
    }

    public Map<QuestionGroup, Integer> getGroupCounts() {
        return Collections.unmodifiableMap(groupCounts);
    }

    public Map<QuestionGroup, List<QuestionItemId>> getRemainingQuestions() {
        Map<QuestionGroup, List<QuestionItemId>> copy = new HashMap<>();
        for (Map.Entry<QuestionGroup, List<QuestionItemId>> entry : remainingQuestions.entrySet()) {
            copy.put(entry.getKey(), Collections.unmodifiableList(entry.getValue()));
        }
        return Collections.unmodifiableMap(copy);
    }

    public Map<QuestionGroup, Integer> getSelectedCounts() {
        return Collections.unmodifiableMap(selectedCounts);
    }

    public int getGroupIndex() {
        return groupIndex;
    }
}
