package me.go_gradually.omypic.presentation.question.dto;

import java.util.ArrayList;
import java.util.List;

public class QuestionGroupRequest {
    private String name;
    private List<String> tags = new ArrayList<>();

    public String getName() {
        return name;
    }

    public void setName(String name) {
        this.name = name;
    }

    public List<String> getTags() {
        return tags;
    }

    public void setTags(List<String> tags) {
        this.tags = tags == null ? new ArrayList<>() : new ArrayList<>(tags);
    }
}
