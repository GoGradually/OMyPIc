package me.go_gradually.omypic.domain.shared.util;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class TextUtilsTest {

    @Test
    void trimToLength_returnsOriginalWhenShortEnough() {
        assertEquals("abc", TextUtils.trimToLength("abc", 5));
    }

    @Test
    void trimToLength_truncatesAtMaxMinusOne() {
        assertEquals("abcd", TextUtils.trimToLength("abcdef", 5));
    }

    @Test
    void trimToLength_returnsEmptyForNull() {
        assertEquals("", TextUtils.trimToLength(null, 5));
    }

    @Test
    void splitChunks_returnsEmptyForBlankText() {
        assertTrue(TextUtils.splitChunks("   ", 3).isEmpty());
    }

    @Test
    void splitChunks_splitsTextIntoFixedSizeChunks() {
        List<String> chunks = TextUtils.splitChunks("abcdefghij", 4);

        assertEquals(List.of("abcd", "efgh", "ij"), chunks);
    }
}
