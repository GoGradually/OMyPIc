package me.go_gradually.omypic.presentation.shared.error;

import me.go_gradually.omypic.application.session.model.InvalidGroupTagsException;
import org.springframework.http.HttpStatus;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestControllerAdvice;

import java.util.Map;
import java.util.NoSuchElementException;

@RestControllerAdvice
public class ApiExceptionHandler {
    @ExceptionHandler(InvalidGroupTagsException.class)
    @ResponseStatus(HttpStatus.BAD_REQUEST)
    public Map<String, Object> invalidGroupTags(InvalidGroupTagsException e) {
        return Map.of(
                "code", "INVALID_GROUP_TAGS",
                "message", e.getMessage() == null ? "Invalid group tags" : e.getMessage(),
                "invalidTags", e.getInvalidTags()
        );
    }

    @ExceptionHandler({IllegalArgumentException.class, IllegalStateException.class})
    @ResponseStatus(HttpStatus.BAD_REQUEST)
    public Map<String, String> badRequest(Exception e) {
        return Map.of("message", e.getMessage() == null ? "Bad request" : e.getMessage());
    }

    @ExceptionHandler(NoSuchElementException.class)
    @ResponseStatus(HttpStatus.NOT_FOUND)
    public Map<String, String> notFound(Exception e) {
        return Map.of("message", e.getMessage() == null ? "Not found" : e.getMessage());
    }
}
