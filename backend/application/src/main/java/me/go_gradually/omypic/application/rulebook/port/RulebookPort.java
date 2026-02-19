package me.go_gradually.omypic.application.rulebook.port;

import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookId;

import java.util.List;
import java.util.Optional;

/**
 * 메타데이터 저장소 인터페이스
 * 트랜잭션을 지원하는 저장소
 */
public interface RulebookPort {
    List<Rulebook> findAll();

    Optional<Rulebook> findById(RulebookId id);

    Rulebook save(Rulebook rulebook);

    void deleteById(RulebookId id);

    void deleteAll();
}
