package me.go_gradually.omypic.presentation.session.controller;

import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.domain.session.SessionState;
import me.go_gradually.omypic.presentation.session.dto.ModeUpdateRequest;
import me.go_gradually.omypic.presentation.session.dto.SessionStateResponse;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

import java.util.Map;
import java.util.stream.Collectors;

@RestController
@RequestMapping("/api/modes")
public class ModeController {
    private final SessionUseCase sessionUseCase;

    public ModeController(SessionUseCase sessionUseCase) {
        this.sessionUseCase = sessionUseCase;
    }

    @PutMapping
    public SessionStateResponse update(@RequestBody ModeUpdateRequest request) {
        if (request.getListId() == null) {
            return toResponse(sessionUseCase.getOrCreate(request.getSessionId()));
        }
        return toResponse(sessionUseCase.updateMode(toCommand(request)));
    }

    private ModeUpdateCommand toCommand(ModeUpdateRequest request) {
        ModeUpdateCommand command = new ModeUpdateCommand();
        command.setSessionId(request.getSessionId());
        command.setListId(request.getListId());
        command.setMode(request.getMode());
        command.setContinuousBatchSize(request.getContinuousBatchSize());
        command.setMockGroupOrder(request.getMockGroupOrder());
        command.setMockGroupCounts(request.getMockGroupCounts());
        return command;
    }

    private SessionStateResponse toResponse(SessionState state) {
        SessionStateResponse response = new SessionStateResponse();
        response.setSessionId(state.getSessionId().value());
        response.setMode(state.getMode());
        response.setContinuousBatchSize(state.getContinuousBatchSize());
        response.setAnsweredSinceLastFeedback(state.getAnsweredSinceLastFeedback());
        response.setSttSegments(state.getSttSegments());
        response.setFeedbackLanguage(state.getFeedbackLanguage().value());
        response.setListIndices(state.getListIndices().entrySet().stream()
                .collect(Collectors.toMap(entry -> entry.getKey().value(), Map.Entry::getValue)));
        response.setMockExamState(toMockResponse(state.getMockExamState()));
        return response;
    }

    private SessionStateResponse.MockExamStateResponse toMockResponse(SessionState.MockExamState mockExamState) {
        if (mockExamState == null) {
            return null;
        }
        SessionStateResponse.MockExamStateResponse response = new SessionStateResponse.MockExamStateResponse();
        response.setGroupOrder(mockExamState.getGroupOrder());
        response.setGroupCounts(mockExamState.getGroupCounts());
        response.setSelectedCounts(mockExamState.getSelectedCounts());
        response.setGroupIndex(mockExamState.getGroupIndex());
        response.setRemainingQuestions(mockExamState.getRemainingQuestions().entrySet().stream()
                .collect(Collectors.toMap(Map.Entry::getKey,
                        entry -> entry.getValue().stream().map(id -> id.value()).toList())));
        return response;
    }
}
