package me.go_gradually.omypic.presentation.question.controller;

import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.presentation.question.dto.NextQuestionResponse;
import me.go_gradually.omypic.presentation.question.dto.QuestionItemRequest;
import me.go_gradually.omypic.presentation.question.dto.QuestionItemResponse;
import me.go_gradually.omypic.presentation.question.dto.QuestionListResponse;
import org.springframework.web.bind.annotation.*;

import java.util.List;
import java.util.Map;

@RestController
@RequestMapping("/api/questions")
public class QuestionController {
    private final QuestionUseCase service;

    public QuestionController(QuestionUseCase service) {
        this.service = service;
    }

    @GetMapping
    public List<QuestionListResponse> list() {
        return service.list().stream()
                .map(this::toResponse)
                .toList();
    }

    @PostMapping
    public QuestionListResponse create(@RequestBody Map<String, String> body) {
        return toResponse(service.create(body.getOrDefault("name", "Untitled")));
    }

    @PutMapping("/{id}")
    public QuestionListResponse rename(@PathVariable("id") String id, @RequestBody Map<String, String> body) {
        return toResponse(service.updateName(id, body.getOrDefault("name", "Untitled")));
    }

    @DeleteMapping("/{id}")
    public void delete(@PathVariable("id") String id) {
        service.delete(id);
    }

    @PostMapping("/{id}/items")
    public QuestionListResponse addQuestion(@PathVariable("id") String id, @RequestBody QuestionItemRequest item) {
        return toResponse(service.addQuestion(id, item.getText(), item.getGroup()));
    }

    @PutMapping("/{id}/items/{itemId}")
    public QuestionListResponse updateQuestion(@PathVariable("id") String id,
                                               @PathVariable("itemId") String itemId,
                                               @RequestBody QuestionItemRequest item) {
        return toResponse(service.updateQuestion(id, itemId, item.getText(), item.getGroup()));
    }

    @DeleteMapping("/{id}/items/{itemId}")
    public QuestionListResponse deleteQuestion(@PathVariable("id") String id, @PathVariable("itemId") String itemId) {
        return toResponse(service.deleteQuestion(id, itemId));
    }

    @GetMapping("/{id}/next")
    public NextQuestionResponse nextQuestion(@PathVariable("id") String id, @RequestParam("sessionId") String sessionId) {
        return toResponse(service.nextQuestion(id, sessionId));
    }

    private NextQuestionResponse toResponse(NextQuestion result) {
        NextQuestionResponse response = new NextQuestionResponse();
        response.setQuestionId(result.getQuestionId());
        response.setText(result.getText());
        response.setGroup(result.getGroup());
        response.setSkipped(result.isSkipped());
        response.setMockExamCompleted(result.isMockExamCompleted());
        response.setMockExamCompletionReason(result.getMockExamCompletionReason());
        return response;
    }

    private QuestionListResponse toResponse(QuestionList list) {
        QuestionListResponse response = new QuestionListResponse();
        response.setId(list.getId().value());
        response.setName(list.getName());
        response.setQuestions(list.getQuestions().stream()
                .map(this::toResponse)
                .toList());
        response.setCreatedAt(list.getCreatedAt());
        response.setUpdatedAt(list.getUpdatedAt());
        return response;
    }

    private QuestionItemResponse toResponse(me.go_gradually.omypic.domain.question.QuestionItem item) {
        QuestionItemResponse response = new QuestionItemResponse();
        response.setId(item.getId().value());
        response.setText(item.getText());
        response.setGroup(item.getGroup() == null ? null : item.getGroup().value());
        return response;
    }
}
