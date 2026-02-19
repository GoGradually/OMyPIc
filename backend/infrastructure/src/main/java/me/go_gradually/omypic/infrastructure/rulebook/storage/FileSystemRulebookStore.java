package me.go_gradually.omypic.infrastructure.rulebook.storage;

import me.go_gradually.omypic.application.rulebook.model.StoredRulebookFile;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import org.springframework.stereotype.Component;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;

@Component
public class FileSystemRulebookStore implements RulebookFileStore {
    private final DataDirProvider dataDirProvider;

    public FileSystemRulebookStore(DataDirProvider dataDirProvider) {
        this.dataDirProvider = dataDirProvider;
    }

    @Override
    public StoredRulebookFile store(String filename, byte[] bytes) throws IOException {
        Path rulebookDir = Path.of(dataDirProvider.getDataDir(), "rulebooks");
        Files.createDirectories(rulebookDir);
        String storedName = System.currentTimeMillis() + "_" + filename;
        Path target = rulebookDir.resolve(storedName);
        Files.write(target, bytes);
        return new StoredRulebookFile(target.toString());
    }

    @Override
    public String readText(String path) throws IOException {
        return Files.readString(Path.of(path), StandardCharsets.UTF_8);
    }

    @Override
    public byte[] readBytes(String path) throws IOException {
        return Files.readAllBytes(Path.of(path));
    }

    @Override
    public void clearAll() throws IOException {
        Path rulebookDir = Path.of(dataDirProvider.getDataDir(), "rulebooks");
        if (!Files.exists(rulebookDir)) {
            return;
        }
        try (var walk = Files.walk(rulebookDir)) {
            walk.sorted(Comparator.reverseOrder())
                    .filter(path -> !path.equals(rulebookDir))
                    .forEach(this::deleteQuietly);
        }
    }

    private void deleteQuietly(Path path) {
        try {
            Files.deleteIfExists(path);
        } catch (IOException e) {
            throw new IllegalStateException("Failed to clear rulebook path: " + path, e);
        }
    }
}
