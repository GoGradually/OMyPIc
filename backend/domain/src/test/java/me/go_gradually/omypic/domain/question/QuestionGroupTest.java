package me.go_gradually.omypic.domain.question;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

class QuestionGroupTest {

    @Test
    void of_trimsValue() {
        QuestionGroup group = QuestionGroup.of("  Group A  ");

        assertEquals("Group A", group.value());
    }

    @Test
    void of_throwsWhenBlank() {
        assertThrows(IllegalArgumentException.class, () -> QuestionGroup.of("  "));
    }

    @Test
    void fromNullable_returnsNullForBlank() {
        assertNull(QuestionGroup.fromNullable(null));
        assertNull(QuestionGroup.fromNullable(" "));
    }

    @Test
    void equalsAndHashCode_useValue() {
        QuestionGroup left = QuestionGroup.of("A");
        QuestionGroup right = QuestionGroup.of("A");

        assertEquals(left, right);
        assertEquals(left.hashCode(), right.hashCode());
    }
}
