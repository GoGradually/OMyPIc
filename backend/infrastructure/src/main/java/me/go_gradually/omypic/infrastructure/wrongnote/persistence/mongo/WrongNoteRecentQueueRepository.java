package me.go_gradually.omypic.infrastructure.wrongnote.persistence.mongo;

import org.springframework.data.mongodb.repository.MongoRepository;

public interface WrongNoteRecentQueueRepository extends MongoRepository<WrongNoteRecentQueueDocument, String> {
}
