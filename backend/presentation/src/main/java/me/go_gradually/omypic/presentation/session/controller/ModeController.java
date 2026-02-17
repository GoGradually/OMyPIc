package me.go_gradually.omypic.presentation.session.controller;

import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.usecase.SessionUseCase;
import me.go_gradually.omypic.presentation.session.dto.ModeUpdateRequest;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/modes")
public class ModeController {
    private final SessionUseCase sessionUseCase;

    public ModeController(SessionUseCase sessionUseCase) {
        this.sessionUseCase = sessionUseCase;
    }

    @PutMapping
    @ResponseStatus(HttpStatus.NO_CONTENT)
    public void update(@RequestBody ModeUpdateRequest request) {
        sessionUseCase.updateMode(toCommand(request));
    }

    private ModeUpdateCommand toCommand(ModeUpdateRequest request) {
        ModeUpdateCommand command = new ModeUpdateCommand();
        command.setSessionId(request.getSessionId());
        command.setMode(request.getMode());
        command.setContinuousBatchSize(request.getContinuousBatchSize());
        command.setSelectedGroupTags(request.getSelectedGroupTags());
        return command;
    }
}
