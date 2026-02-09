package me.go_gradually.omypic.infrastructure.question.persistence.mongo;

import org.springframework.data.mongodb.repository.MongoRepository;

public interface QuestionListMongoRepository extends MongoRepository<QuestionListDocument, String> {
}
