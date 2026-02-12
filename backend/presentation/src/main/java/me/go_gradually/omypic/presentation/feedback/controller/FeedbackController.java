package me.go_gradually.omypic.presentation.feedback.controller;

import jakarta.validation.Valid;
import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.presentation.feedback.dto.FeedbackEnvelope;
import me.go_gradually.omypic.presentation.feedback.dto.FeedbackRequest;
import me.go_gradually.omypic.presentation.feedback.dto.FeedbackResponse;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.server.ResponseStatusException;

@RestController
@RequestMapping("/api/feedback")
public class FeedbackController {
    private final FeedbackUseCase feedbackUseCase;
    private final boolean restDisabled;

    public FeedbackController(FeedbackUseCase feedbackUseCase,
                              @Value("${omypic.realtime.rest-disabled:true}") boolean restDisabled) {
        this.feedbackUseCase = feedbackUseCase;
        this.restDisabled = restDisabled;
    }

    @PostMapping
    public FeedbackEnvelope feedback(@RequestHeader("X-API-Key") String apiKey,
                                     @Valid @RequestBody FeedbackRequest request) {
        assertLegacyRouteEnabled();
        FeedbackResult result = feedbackUseCase.generateFeedback(apiKey, toCommand(request));
        if (!result.isGenerated()) {
            return FeedbackEnvelope.skipped();
        }
        return FeedbackEnvelope.generated(toResponse(result.getFeedback()));
    }

    private FeedbackCommand toCommand(FeedbackRequest request) {
        FeedbackCommand command = new FeedbackCommand();
        command.setSessionId(request.getSessionId());
        command.setText(request.getText());
        command.setProvider(request.getProvider());
        command.setModel(request.getModel());
        command.setFeedbackLanguage(request.getFeedbackLanguage());
        return command;
    }

    private FeedbackResponse toResponse(Feedback feedback) {
        FeedbackResponse response = new FeedbackResponse();
        response.setSummary(feedback.getSummary());
        response.setCorrectionPoints(feedback.getCorrectionPoints());
        response.setExampleAnswer(feedback.getExampleAnswer());
        response.setRulebookEvidence(feedback.getRulebookEvidence());
        return response;
    }

    private void assertLegacyRouteEnabled() {
        if (restDisabled) {
            throw new ResponseStatusException(HttpStatus.GONE, "Use websocket /api/realtime/voice");
        }
    }
}
