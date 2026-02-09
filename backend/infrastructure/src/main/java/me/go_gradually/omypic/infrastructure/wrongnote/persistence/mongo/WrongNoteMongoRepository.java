package me.go_gradually.omypic.infrastructure.wrongnote.persistence.mongo;

import org.springframework.data.mongodb.repository.MongoRepository;

import java.util.Optional;

public interface WrongNoteMongoRepository extends MongoRepository<WrongNoteDocument, String> {
    Optional<WrongNoteDocument> findByPattern(String pattern);
}
