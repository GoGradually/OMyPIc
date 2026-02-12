package me.go_gradually.omypic.application.session.usecase;

import me.go_gradually.omypic.application.question.port.QuestionListPort;
import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;

import java.util.List;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Optional;

public class SessionUseCase {
    private final SessionStorePort sessionStore;
    private final QuestionListPort questionListPort;

    public SessionUseCase(SessionStorePort sessionStore, QuestionListPort questionListPort) {
        this.sessionStore = sessionStore;
        this.questionListPort = questionListPort;
    }

    public SessionState getOrCreate(String sessionId) {
        return sessionStore.getOrCreate(SessionId.of(sessionId));
    }

    public SessionState updateMode(ModeUpdateCommand command) {
        SessionState state = getOrCreate(command.getSessionId());
        state.applyModeUpdate(command.getMode(), command.getContinuousBatchSize());
        if (command.getListId() == null) {
            return state;
        }
        QuestionList list = questionListPort.findById(QuestionListId.of(command.getListId())).orElseThrow();
        state.setActiveQuestionListId(command.getListId());
        state.resetQuestionProgress(command.getListId());
        if (state.getMode() == ModeType.MOCK_EXAM) {
            ModeUpdateCommand.MockPlan mockPlan = command.getMockPlan();
            List<QuestionGroup> order = Optional.ofNullable(mockPlan)
                    .map(ModeUpdateCommand.MockPlan::getGroupOrder)
                    .orElse(List.of())
                    .stream()
                    .map(QuestionGroup::of)
                    .toList();
            Map<QuestionGroup, Integer> counts = new LinkedHashMap<>();
            Optional.ofNullable(mockPlan)
                    .map(ModeUpdateCommand.MockPlan::getGroupCounts)
                    .orElse(Map.of())
                    .forEach((key, value) -> counts.put(QuestionGroup.of(key), value));
            state.configureMockExam(list, order, counts);
        }
        return state;
    }

    public void appendSegment(String sessionId, String text) {
        SessionState state = getOrCreate(sessionId);
        state.appendSegment(text);
    }
}
