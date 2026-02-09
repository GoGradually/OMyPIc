package me.go_gradually.omypic.infrastructure.rulebook.persistence.mongo;

import org.springframework.data.mongodb.repository.MongoRepository;

public interface RulebookMongoRepository extends MongoRepository<RulebookDocument, String> {
}
