package me.go_gradually.omypic.application.wrongnote.port;

import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteId;

import java.util.List;
import java.util.Optional;

public interface WrongNotePort {
    List<WrongNote> findAll();

    Optional<WrongNote> findByPattern(String pattern);

    WrongNote save(WrongNote note);

    void deleteById(WrongNoteId id);
}
