package me.go_gradually.omypic.infrastructure.rulebook.storage;

import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class FileSystemRulebookStoreTest {

    @TempDir
    Path tempDir;

    @Test
    void store_writesFileUnderRulebookDirectory() throws Exception {
        FileSystemRulebookStore store = new FileSystemRulebookStore(dataDir(tempDir));

        byte[] bytes = "hello markdown".getBytes();
        String storedPath = store.store("rules.md", bytes).path();

        Path path = Path.of(storedPath);
        assertTrue(path.startsWith(tempDir.resolve("rulebooks")));
        assertTrue(Files.exists(path));
        assertEquals("hello markdown", Files.readString(path));
    }

    @Test
    void readText_readsUtf8Text() throws Exception {
        FileSystemRulebookStore store = new FileSystemRulebookStore(dataDir(tempDir));
        Path file = tempDir.resolve("manual.md");
        Files.writeString(file, "plain ascii text");

        String text = store.readText(file.toString());

        assertEquals("plain ascii text", text);
    }

    private DataDirProvider dataDir(Path path) {
        return () -> path.toString();
    }
}
