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

@RestController
@RequestMapping("/api/modes")
public class ModeController {
    private final SessionUseCase sessionUseCase;

    public ModeController(SessionUseCase sessionUseCase) {
        this.sessionUseCase = sessionUseCase;
    }

    @PutMapping
    public SessionStateResponse update(@RequestBody ModeUpdateRequest request) {
        return toResponse(sessionUseCase.updateMode(toCommand(request)));
    }

    private ModeUpdateCommand toCommand(ModeUpdateRequest request) {
        ModeUpdateCommand command = new ModeUpdateCommand();
        command.setSessionId(request.getSessionId());
        command.setMode(request.getMode());
        command.setContinuousBatchSize(request.getContinuousBatchSize());
        command.setSelectedGroupTags(request.getSelectedGroupTags());
        return command;
    }

    private SessionStateResponse toResponse(SessionState state) {
        SessionStateResponse response = new SessionStateResponse();
        response.setSessionId(state.getSessionId().value());
        response.setMode(state.getMode());
        response.setContinuousBatchSize(state.getContinuousBatchSize());
        response.setCompletedGroupCountSinceLastFeedback(state.getCompletedGroupCountSinceLastFeedback());
        response.setSelectedGroupTags(state.getSelectedGroupTags());
        response.setCandidateGroupOrder(state.getCandidateGroupOrder());
        response.setGroupQuestionIndices(state.getGroupQuestionIndices());
        response.setSttSegments(state.getSttSegments());
        response.setFeedbackLanguage(state.getFeedbackLanguage().value());
        return response;
    }
}
