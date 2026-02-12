package me.go_gradually.omypic.application.session.model;

import java.util.ArrayList;
import java.util.List;

public class InvalidGroupTagsException extends IllegalArgumentException {
    private final List<String> invalidTags;

    public InvalidGroupTagsException(String message, List<String> invalidTags) {
        super(message);
        this.invalidTags = invalidTags == null ? List.of() : List.copyOf(new ArrayList<>(invalidTags));
    }

    public List<String> getInvalidTags() {
        return invalidTags;
    }
}
