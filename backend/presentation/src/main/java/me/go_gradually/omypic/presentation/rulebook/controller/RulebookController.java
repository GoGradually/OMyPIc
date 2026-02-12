package me.go_gradually.omypic.presentation.rulebook.controller;

import me.go_gradually.omypic.application.rulebook.usecase.RulebookUseCase;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookScope;
import me.go_gradually.omypic.presentation.rulebook.dto.RulebookResponse;
import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.multipart.MultipartFile;

import java.io.IOException;
import java.util.List;

@RestController
@RequestMapping("/api/rulebooks")
public class RulebookController {
    private final RulebookUseCase service;

    public RulebookController(RulebookUseCase service) {
        this.service = service;
    }

    @GetMapping
    public List<RulebookResponse> list() {
        return service.list().stream()
                .map(this::toResponse)
                .toList();
    }

    @PostMapping(consumes = MediaType.MULTIPART_FORM_DATA_VALUE)
    public RulebookResponse upload(@RequestParam("file") MultipartFile file,
                                   @RequestParam(value = "scope", required = false) RulebookScope scope,
                                   @RequestParam(value = "questionGroup", required = false) String questionGroup) throws IOException {
        return toResponse(service.upload(file.getOriginalFilename(), file.getBytes(), scope, questionGroup));
    }

    @PutMapping("/{id}/toggle")
    public RulebookResponse toggle(@PathVariable("id") String id, @RequestParam("enabled") boolean enabled) {
        return toResponse(service.toggle(id, enabled));
    }

    @DeleteMapping("/{id}")
    public void delete(@PathVariable("id") String id) {
        service.delete(id);
    }

    private RulebookResponse toResponse(Rulebook rulebook) {
        RulebookResponse response = new RulebookResponse();
        response.setId(rulebook.getId().value());
        response.setFilename(rulebook.getFilename());
        response.setPath(rulebook.getPath());
        response.setScope(rulebook.getScope());
        response.setQuestionGroup(rulebook.getQuestionGroup() == null ? null : rulebook.getQuestionGroup().value());
        response.setEnabled(rulebook.isEnabled());
        response.setCreatedAt(rulebook.getCreatedAt());
        response.setUpdatedAt(rulebook.getUpdatedAt());
        return response;
    }
}
