package me.go_gradually.omypic.presentation.question.dto;

import java.time.Instant;
import java.util.List;

public class QuestionListResponse {
    private String id;
    private String name;
    private List<QuestionItemResponse> questions;
    private Instant createdAt;
    private Instant updatedAt;

    public String getId() {
        return id;
    }

    public void setId(String id) {
        this.id = id;
    }

    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public List<QuestionItemResponse> getQuestions() {
        return questions;
    }

    public void setQuestions(List<QuestionItemResponse> questions) {
        this.questions = questions;
    }

    public Instant getCreatedAt() {
        return createdAt;
    }

    public void setCreatedAt(Instant createdAt) {
        this.createdAt = createdAt;
    }

    public Instant getUpdatedAt() {
        return updatedAt;
    }

    public void setUpdatedAt(Instant updatedAt) {
        this.updatedAt = updatedAt;
    }
}
