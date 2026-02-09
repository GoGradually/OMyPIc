package me.go_gradually.omypic.application.rulebook.port;

import me.go_gradually.omypic.application.rulebook.model.StoredRulebookFile;

import java.io.IOException;

// 트랜잭션을 지원받지 못하는 파일 저장소 인터페이스
public interface RulebookFileStore {
    StoredRulebookFile store(String filename, byte[] bytes) throws IOException;

    String readText(String path) throws IOException;
}
