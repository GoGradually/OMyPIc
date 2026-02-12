package me.go_gradually.omypic.presentation.question.controller;

import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.model.QuestionTagStat;
import me.go_gradually.omypic.application.question.usecase.QuestionUseCase;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.presentation.question.dto.NextQuestionResponse;
import me.go_gradually.omypic.presentation.question.dto.QuestionGroupRequest;
import me.go_gradually.omypic.presentation.question.dto.QuestionGroupResponse;
import me.go_gradually.omypic.presentation.question.dto.QuestionItemRequest;
import me.go_gradually.omypic.presentation.question.dto.QuestionItemResponse;
import me.go_gradually.omypic.presentation.question.dto.QuestionTagStatResponse;
import org.springframework.web.bind.annotation.*;

import java.util.List;

@RestController
@RequestMapping("/api/question-groups")
public class QuestionController {
    private final QuestionUseCase service;

    public QuestionController(QuestionUseCase service) {
        this.service = service;
    }

    @GetMapping
    public List<QuestionGroupResponse> list() {
        return service.list().stream()
                .map(this::toResponse)
                .toList();
    }

    @PostMapping
    public QuestionGroupResponse create(@RequestBody QuestionGroupRequest body) {
        return toResponse(service.createGroup(body.getName(), body.getTags()));
    }

    @PutMapping("/{groupId}")
    public QuestionGroupResponse updateGroup(@PathVariable("groupId") String groupId,
                                             @RequestBody QuestionGroupRequest body) {
        return toResponse(service.updateGroup(groupId, body.getName(), body.getTags()));
    }

    @DeleteMapping("/{groupId}")
    public void deleteGroup(@PathVariable("groupId") String groupId) {
        service.deleteGroup(groupId);
    }

    @PostMapping("/{groupId}/items")
    public QuestionGroupResponse addQuestion(@PathVariable("groupId") String groupId,
                                             @RequestBody QuestionItemRequest item) {
        return toResponse(service.addQuestion(groupId, item.getText(), item.getQuestionType()));
    }

    @PutMapping("/{groupId}/items/{itemId}")
    public QuestionGroupResponse updateQuestion(@PathVariable("groupId") String groupId,
                                                @PathVariable("itemId") String itemId,
                                                @RequestBody QuestionItemRequest item) {
        return toResponse(service.updateQuestion(groupId, itemId, item.getText(), item.getQuestionType()));
    }

    @DeleteMapping("/{groupId}/items/{itemId}")
    public QuestionGroupResponse deleteQuestion(@PathVariable("groupId") String groupId,
                                                @PathVariable("itemId") String itemId) {
        return toResponse(service.deleteQuestion(groupId, itemId));
    }

    @GetMapping("/next")
    public NextQuestionResponse nextQuestion(@RequestParam("sessionId") String sessionId) {
        return toResponse(service.nextQuestion(sessionId));
    }

    @GetMapping("/tags/stats")
    public List<QuestionTagStatResponse> tagStats() {
        return service.listTagStats().stream()
                .map(this::toResponse)
                .toList();
    }

    private NextQuestionResponse toResponse(NextQuestion result) {
        NextQuestionResponse response = new NextQuestionResponse();
        response.setQuestionId(result.getQuestionId());
        response.setText(result.getText());
        response.setGroupId(result.getGroupId());
        response.setGroup(result.getGroup());
        response.setQuestionType(result.getQuestionType());
        response.setSkipped(result.isSkipped());
        return response;
    }

    private QuestionTagStatResponse toResponse(QuestionTagStat stat) {
        QuestionTagStatResponse response = new QuestionTagStatResponse();
        response.setTag(stat.tag());
        response.setGroupCount(stat.groupCount());
        response.setSelectable(stat.selectable());
        return response;
    }

    private QuestionGroupResponse toResponse(QuestionGroupAggregate group) {
        QuestionGroupResponse response = new QuestionGroupResponse();
        response.setId(group.getId().value());
        response.setName(group.getName());
        response.setTags(group.getTags().stream().sorted().toList());
        response.setQuestions(group.getQuestions().stream()
                .map(this::toResponse)
                .toList());
        response.setCreatedAt(group.getCreatedAt());
        response.setUpdatedAt(group.getUpdatedAt());
        return response;
    }

    private QuestionItemResponse toResponse(me.go_gradually.omypic.domain.question.QuestionItem item) {
        QuestionItemResponse response = new QuestionItemResponse();
        response.setId(item.getId().value());
        response.setText(item.getText());
        response.setQuestionType(item.getQuestionType());
        return response;
    }
}
