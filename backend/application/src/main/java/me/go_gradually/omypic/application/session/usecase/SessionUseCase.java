package me.go_gradually.omypic.application.session.usecase;

import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.application.session.model.InvalidGroupTagsException;
import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;

public class SessionUseCase {
    private final SessionStorePort sessionStore;
    private final QuestionGroupPort questionGroupPort;

    public SessionUseCase(SessionStorePort sessionStore, QuestionGroupPort questionGroupPort) {
        this.sessionStore = sessionStore;
        this.questionGroupPort = questionGroupPort;
    }

    public SessionState getOrCreate(String sessionId) {
        return sessionStore.getOrCreate(SessionId.of(sessionId));
    }

    public SessionState updateMode(ModeUpdateCommand command) {
        SessionState state = getOrCreate(command.getSessionId());
        state.applyModeUpdate(command.getMode(), command.getContinuousBatchSize());
        Set<String> selectedTags = normalizedSelectedTags(command);
        List<QuestionGroupAggregate> allGroups = questionGroupPort.findAll();
        validateSelectedTags(selectedTags, allGroups);
        List<String> candidateGroupIds = shuffledCandidateGroupIds(allGroups, selectedTags);
        state.configureQuestionGroups(selectedTags, candidateGroupIds);
        return state;
    }

    private Set<String> normalizedSelectedTags(ModeUpdateCommand command) {
        Set<String> selectedTags = QuestionGroupAggregate.normalizeTags(command.getSelectedGroupTags());
        if (selectedTags.isEmpty()) {
            throw new InvalidGroupTagsException("selectedGroupTags must not be empty", List.of());
        }
        return selectedTags;
    }

    private void validateSelectedTags(Set<String> selectedTags, List<QuestionGroupAggregate> allGroups) {
        Set<String> availableTags = availableTags(allGroups);
        List<String> invalidTags = invalidTags(selectedTags, availableTags);
        if (!invalidTags.isEmpty()) {
            throw new InvalidGroupTagsException("Some selectedGroupTags are invalid", invalidTags);
        }
    }

    private Set<String> availableTags(List<QuestionGroupAggregate> allGroups) {
        return allGroups.stream().flatMap(group -> group.getTags().stream()).collect(Collectors.toSet());
    }

    private List<String> invalidTags(Set<String> selectedTags, Set<String> availableTags) {
        return selectedTags.stream()
                .filter(tag -> !availableTags.contains(tag))
                .sorted()
                .toList();
    }

    private List<String> shuffledCandidateGroupIds(List<QuestionGroupAggregate> allGroups, Set<String> selectedTags) {
        List<String> candidateGroupIds = allGroups.stream()
                .filter(group -> group.hasAnyTag(selectedTags))
                .filter(QuestionGroupAggregate::hasQuestions)
                .map(group -> group.getId().value())
                .collect(Collectors.toCollection(ArrayList::new));
        Collections.shuffle(candidateGroupIds);
        return candidateGroupIds;
    }

    public void appendSegment(String sessionId, String text) {
        SessionState state = getOrCreate(sessionId);
        state.appendSegment(text);
    }
}
