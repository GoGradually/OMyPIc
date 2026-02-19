package me.go_gradually.omypic.infrastructure.wrongnote.persistence.mongo;

import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteId;
import org.springframework.stereotype.Component;

import java.util.List;
import java.util.Optional;
import java.util.stream.Collectors;

@Component
public class WrongNoteMongoAdapter implements WrongNotePort {
    private final WrongNoteMongoRepository repository;

    public WrongNoteMongoAdapter(WrongNoteMongoRepository repository) {
        this.repository = repository;
    }

    @Override
    public List<WrongNote> findAll() {
        return repository.findAll().stream()
                .map(this::toDomain)
                .collect(Collectors.toList());
    }

    @Override
    public Optional<WrongNote> findByPattern(String pattern) {
        return repository.findByPattern(pattern).map(this::toDomain);
    }

    @Override
    public WrongNote save(WrongNote note) {
        WrongNoteDocument saved = repository.save(toDocument(note));
        return toDomain(saved);
    }

    @Override
    public void deleteById(WrongNoteId id) {
        repository.deleteById(id.value());
    }

    @Override
    public void deleteAll() {
        repository.deleteAll();
    }

    private WrongNote toDomain(WrongNoteDocument doc) {
        return WrongNote.rehydrate(
                WrongNoteId.of(doc.getId()),
                doc.getPattern(),
                doc.getCount(),
                doc.getShortSummary(),
                doc.getLastSeenAt()
        );
    }

    private WrongNoteDocument toDocument(WrongNote note) {
        WrongNoteDocument doc = new WrongNoteDocument();
        doc.setId(note.getId().value());
        doc.setPattern(note.getPattern());
        doc.setCount(note.getCount());
        doc.setShortSummary(note.getShortSummary());
        doc.setLastSeenAt(note.getLastSeenAt());
        return doc;
    }
}
