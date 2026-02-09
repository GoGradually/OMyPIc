package me.go_gradually.omypic.presentation.feedback.controller;

import jakarta.validation.Valid;
import me.go_gradually.omypic.application.feedback.model.FeedbackCommand;
import me.go_gradually.omypic.application.feedback.model.FeedbackResult;
import me.go_gradually.omypic.application.feedback.usecase.FeedbackUseCase;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.presentation.feedback.dto.FeedbackEnvelope;
import me.go_gradually.omypic.presentation.feedback.dto.FeedbackRequest;
import me.go_gradually.omypic.presentation.feedback.dto.FeedbackResponse;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/feedback")
public class FeedbackController {
    private final FeedbackUseCase feedbackUseCase;

    public FeedbackController(FeedbackUseCase feedbackUseCase) {
        this.feedbackUseCase = feedbackUseCase;
    }

    @PostMapping
    public FeedbackEnvelope feedback(@RequestHeader("X-API-Key") String apiKey,
                                     @Valid @RequestBody FeedbackRequest request) {
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
}
