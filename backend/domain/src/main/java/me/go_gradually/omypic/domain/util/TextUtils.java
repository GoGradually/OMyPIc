package me.go_gradually.omypic.domain.util;

import java.util.ArrayList;
import java.util.List;

public final class TextUtils {
    private TextUtils() {
    }

    public static String trimToLength(String text, int maxChars) {
        if (text == null) {
            return "";
        }
        if (text.length() <= maxChars) {
            return text;
        }
        return text.substring(0, Math.max(0, maxChars - 1)).trim();
    }

    public static List<String> splitChunks(String text, int maxChunkSize) {
        List<String> chunks = new ArrayList<>();
        if (text == null || text.isBlank()) {
            return chunks;
        }
        int index = 0;
        while (index < text.length()) {
            int end = Math.min(text.length(), index + maxChunkSize);
            chunks.add(text.substring(index, end));
            index = end;
        }
        return chunks;
    }
}
