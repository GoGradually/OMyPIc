package me.go_gradually.omypic.presentation.wrongnote.controller;

import me.go_gradually.omypic.application.wrongnote.usecase.WrongNoteUseCase;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.presentation.wrongnote.dto.WrongNoteResponse;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

import java.util.List;

@RestController
@RequestMapping("/api/wrongnotes")
public class WrongNoteController {
    private final WrongNoteUseCase service;

    public WrongNoteController(WrongNoteUseCase service) {
        this.service = service;
    }

    @GetMapping
    public List<WrongNoteResponse> list() {
        return service.list().stream()
                .map(this::toResponse)
                .toList();
    }

    private WrongNoteResponse toResponse(WrongNote note) {
        WrongNoteResponse response = new WrongNoteResponse();
        response.setId(note.getId().value());
        response.setPattern(note.getPattern());
        response.setCount(note.getCount());
        response.setShortSummary(note.getShortSummary());
        response.setLastSeenAt(note.getLastSeenAt());
        return response;
    }
}
