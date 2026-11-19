# Rust 백엔드 + SQLite Implementation Plan (1/3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `backend/`의 Spring Boot 백엔드를 같은 HTTP API를 가진 Rust 서버로 옮기고, MongoDB와 Lucene을 SQLite로 바꾼다.

**Architecture:** Java 모듈을 Cargo workspace의 crate로 1:1 대응시킨다(`domain → application → infrastructure → presentation → server`). Java 코드처럼 포트는 동기(blocking) trait이고, axum 핸들러가 유스케이스를 `tokio::task::spawn_blocking`으로 부른다. Java 소스가 동작의 기준이고 Java 테스트가 테스트의 기준이다.

**Tech Stack:** Rust 1.97, axum 0.8, tokio 1, rusqlite(`bundled`), reqwest 0.12(`blocking`, `json`, `multipart`, `rustls-tls`, 기본 기능 끔), serde/serde_json, chrono, uuid, zip, flate2, sha2, thiserror, tracing, tower-http. 테스트: httpmock, tempfile.

**Spec:** `docs/superpowers/specs/2026-11-19-rust-migration-design.md`

## Global Constraints

- HTTP API의 경로, 메서드, 요청/응답 JSON 필드 이름, 상태 코드, SSE 이벤트 이름과 payload는 Java 백엔드와 같아야 한다. 오류 본문은 `{"message": "..."}`이고, 그룹 태그 오류만 `{"code":"INVALID_GROUP_TAGS","message":...,"invalidTags":[...]}`다.
- `OMYPIC_*` 환경 변수의 이름과 기본값은 `backend/bootstrap/src/main/resources/application.yml`과 같다. `OMYPIC_MONGODB_URI`만 없앤다.
- 백업 zip 형식은 그대로다: `schemaVersion` 1, `manifest.json`, `data/question-groups.json`, `data/rulebooks.json`, `data/wrong-notes.json`, `data/wrong-note-queue.json`, `files/rulebooks/…`.
- 서버는 `127.0.0.1`에만 바인딩한다. 기본 포트 4317.
- 이름 규칙: Java 클래스 `Foo` → Rust 타입 `Foo`(같은 이름). Java 패키지 `domain.feedback` → 모듈 `omypic_domain::feedback`. 메서드는 snake_case. 포트 인터페이스는 같은 이름의 `trait … : Send + Sync`이고 `Arc<dyn Trait>`로 주입한다.
- 예외 대응: `IllegalArgumentException` → `AppError::InvalidArgument(String)`, `IllegalStateException` → `AppError::InvalidState(String)`, `NoSuchElementException` → `AppError::NotFound(String)`, `InvalidGroupTagsException` → `AppError::InvalidGroupTags { message, invalid_tags }`. `AppError`는 `omypic_domain::error`에 둔다. 오류 메시지 문자열은 Java와 같게 한다.
- 시각은 `chrono::DateTime<Utc>`. JSON으로 쓸 때는 ISO-8601 문자열(`2026-02-19T00:00:00Z`), 읽을 때는 ISO 문자열과 초 단위 숫자를 모두 받는다.
- Mongo가 만들어 주던 id는 UUID v4 문자열로 만든다. 기존 id(가져온 백업의 값)는 문자열 그대로 보존한다.
- 테스트 대응: Java `@Test` 메서드 하나마다 같은 이름(snake_case)의 Rust 테스트를 만들고 같은 값을 단언한다. Mockito mock은 직접 만든 가짜 구현으로 바꾼다. Java 테스트를 건너뛰거나 단언을 약하게 만들지 않는다. Rust에서 의미가 없는 테스트(Spring 설정 자체를 검사하는 것 등)는 건너뛰되 커밋 메시지에 이유를 적는다.
- Java 소스(`backend/domain` 등)는 Task 13 전까지 지우지 않는다. Rust 코드는 `backend/crates/` 아래에 만든다.
- 커밋 메시지는 기존 형식(`feat: …`, `test: …`, 한국어 본문)을 따른다. 작업마다 커밋한다.
- `cargo clippy --workspace --all-targets -- -D warnings`와 `cargo fmt --check`가 통과해야 한다.

## Review Focus

1. **0.2.x 백업 zip의 변형.** 시각이 숫자(초)로 적힌 파일, 모르는 필드가 있는 파일, 선택 필드가 빠진 파일도 가져와야 한다. → Task 5.
2. **큰 요청 본문.** 음성 청크(JSON 안의 base64)와 룰북/백업 업로드가 axum 기본 제한(2MB)에 걸리면 안 된다. JSON 본문은 64MB, multipart는 64MB까지 받는다. → Task 12.
3. **한글 파일 이름.** 한글 이름의 룰북을 올리고, 내보내고, 다시 가져와도 이름과 내용이 유지되어야 한다. → Task 8, Task 12.
4. **SSE 연결이 끊긴 세션.** 클라이언트가 이벤트 스트림을 닫은 뒤 서버가 이벤트를 보내도 패닉 없이 `send`가 `false`를 돌려주고, 진행 중인 턴이 정리되어야 한다. → Task 12.
5. **SQLite 동시 접근.** 음성 턴 처리 스레드가 오답노트를 쓰는 동안 다른 요청이 질문 목록을 읽어도 "database is locked" 오류가 나지 않아야 한다. → Task 8.

---

## 파일 구조

```
Cargo.toml                                   # [workspace] members = ["backend/crates/*"], resolver = "2"
rust-toolchain.toml                          # channel = "1.97"
backend/crates/
  domain/src/{lib,error}.rs
  domain/src/{feedback,question,rulebook,session,wrongnote,shared}/…
  application/src/{apikey,datatransfer,feedback,question,rulebook,session,shared,stt,voice,wrongnote}/…
  application/src/test_support/…             # feature "test-support": 포트의 가짜 구현
  infrastructure/src/config.rs               # AppProperties 대응
  infrastructure/src/sqlite/{mod,schema,question,rulebook,wrongnote,rulebook_index}.rs
  infrastructure/src/{openai,anthropic,embedding,storage,metrics,session}/…
  presentation/src/{lib,error,state,sse}.rs
  presentation/src/routes/{apikey,datatransfer,meta,question,rulebook,mode,voice,wrongnote,actuator}.rs
  server/src/{lib,main,wiring}.rs
```

crate 이름: `omypic-domain`, `omypic-application`, `omypic-infrastructure`, `omypic-presentation`, `omypic-server`.

아래에서 "Java 경로"는 `backend/<module>/src/{main,test}/java/me/go_gradually/omypic/` 아래의 상대 경로다.

---

### Task 1: Workspace와 domain (question, rulebook, wrongnote, shared)

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `backend/crates/domain/{Cargo.toml,src/lib.rs,src/error.rs}`
- Create: `backend/crates/domain/src/{question,rulebook,wrongnote,shared}/…`
- Modify: `.gitignore` (`target/` 추가)
- Java 기준: `domain/{question,rulebook,wrongnote,shared}/**` (main 13개 파일, test `QuestionGroupTest`, `QuestionListTest`, `RulebookTest`, `TextUtilsTest`, `WrongNoteWindowTest`)

**Interfaces:**
- Produces: `omypic_domain::error::AppError`(Global Constraints의 네 가지), `pub type AppResult<T> = Result<T, AppError>`. Java 타입과 같은 이름의 타입 전부(`QuestionGroupAggregate`, `QuestionGroup`, `QuestionItem`, `QuestionGroupId`, `QuestionItemId`, `Rulebook`, `RulebookId`, `RulebookScope`, `RulebookContext`, `WrongNote`, `WrongNoteId`, `WrongNoteWindow`, `shared::text_utils`). id 타입은 `value() -> &str`, `of(&str) -> AppResult<Self>`를 가진다. domain crate는 `chrono`, `uuid`, `thiserror` 외 의존성을 갖지 않는다.

- [ ] **Step 1:** workspace와 domain crate를 만든다. `cargo build -p omypic-domain`이 통과한다.
- [ ] **Step 2:** 위 Java 테스트 5개 파일의 모든 `@Test`를 Rust 테스트로 옮긴다(각 모듈의 `#[cfg(test)] mod tests`).
- [ ] **Step 3:** `cargo test -p omypic-domain` 실행. 컴파일 실패(타입 없음)를 확인한다.
- [ ] **Step 4:** Java main 파일을 옮긴다. 공개 메서드와 검증 규칙, 오류 메시지를 그대로 둔다.
- [ ] **Step 5:** `cargo test -p omypic-domain` 통과, clippy와 fmt 통과.
- [ ] **Step 6:** 커밋 `feat: Rust workspace와 domain crate(question, rulebook, wrongnote) 추가`.

### Task 2: domain (feedback, session)

**Files:**
- Create: `backend/crates/domain/src/{feedback,session}/…`
- Java 기준: `domain/{feedback,session}/**` (main 15개 파일, test `FeedbackTest`, `SessionFlowPolicyTest`, `SessionStateTest`, `TurnBatchingPolicyTest`)

**Interfaces:**
- Consumes: Task 1의 `AppError`, `text_utils`, `QuestionGroupId` 등.
- Produces: `Feedback`, `Corrections`, `CorrectionDetail`, `Recommendations`, `RecommendationDetail`, `FeedbackConstraints`, `FeedbackLanguage`, `SessionState`, `SessionId`, `ModeType`, `SessionFlowPolicy`, `SessionStopReason`, `TurnBatchingPolicy`, `LlmConversationState`, `LlmPromptContext`. `SessionState`는 Java처럼 가변 객체다. 공유는 application 쪽에서 `Arc<Mutex<SessionState>>`로 한다.

- [ ] **Step 1:** Java 테스트 4개 파일을 Rust 테스트로 옮긴다.
- [ ] **Step 2:** `cargo test -p omypic-domain` 실행, 실패 확인.
- [ ] **Step 3:** Java main 파일을 옮긴다. `Feedback.java`(627줄)의 정규화·길이 제한·중복 제거 규칙은 문자 단위 길이(Java `String.length()`는 UTF-16 단위)까지 같게 한다. 한글과 이모지가 섞인 입력에서 Java와 결과가 달라지지 않도록 길이는 UTF-16 단위로 센다.
- [ ] **Step 4:** 테스트, clippy, fmt 통과.
- [ ] **Step 5:** 커밋 `feat: domain crate에 feedback, session 추가`.

### Task 3: application 포트, 모델, 작은 유스케이스

**Files:**
- Create: `backend/crates/application/{Cargo.toml,src/lib.rs}`, `src/{apikey,question,rulebook,session,shared,stt,wrongnote}/…`, `src/test_support/…`
- Java 기준: `application/{apikey,question,rulebook,session,shared,stt,wrongnote}/**` 와 `application/feedback/{model,port}/**`, `application/voice/{model,port,policy}/**`
- Java 테스트: `ApiKeyVerifyUseCaseTest`, `QuestionUseCaseTest`, `RulebookUseCaseTest`, application 아래 `session`, `stt`, `wrongnote` 패키지의 테스트 전부

**Interfaces:**
- Consumes: domain 전체.
- Produces (모두 동기 trait, 반환은 `AppResult<…>`):
  - 포트: `ApiKeyProbePort`, `QuestionGroupPort`, `RulebookPort`, `RulebookFileStore`, `RulebookIndexPort`, `EmbeddingPort`, `SessionStorePort`, `WrongNotePort`, `WrongNoteRecentQueuePort`, `SttGateway`, `TtsGateway`, `LlmClient`, `MetricsPort`, `DataDirProvider`.
  - `AsyncExecutor`: `fn execute(&self, task: Box<dyn FnOnce() + Send + 'static>)`.
  - `VoiceEventSink`: `fn send(&self, event: &str, payload: serde_json::Value) -> bool`.
  - 정책 trait: `FeedbackPolicy`, `RagPolicy`, `SttPolicy`, `VoicePolicy`(Java의 정책 인터페이스와 같은 getter).
  - 유스케이스: `ApiKeyVerifyUseCase`, `QuestionUseCase`, `RulebookUseCase`, `SessionUseCase`, `SttUseCase`, `WrongNoteUseCase`. 생성자 인자 순서는 Java와 같다.
  - `VoiceModelCatalog`(지금 Java와 같은 목록. `device-voice` 제거는 계획 3에서 한다).
  - `test_support`(Cargo feature `test-support`): 각 포트의 메모리 가짜 구현(`InMemoryQuestionGroupPort` 등), 호출을 기록하는 `RecordingMetrics`, 즉시 실행하는 `ImmediateExecutor`, 이벤트를 모으는 `RecordingSink`. 이후 작업의 테스트가 이것을 쓴다.

- [ ] **Step 1:** 포트, 모델, 정책 trait를 옮긴다(동작이 없는 선언). `cargo build -p omypic-application` 통과.
- [ ] **Step 2:** `test_support`의 가짜 구현을 만든다.
- [ ] **Step 3:** 위 Java 테스트를 Rust로 옮긴다. 실행해서 실패를 확인한다.
- [ ] **Step 4:** 유스케이스를 옮긴다.
- [ ] **Step 5:** `cargo test -p omypic-application` 통과, clippy와 fmt 통과.
- [ ] **Step 6:** 커밋 `feat: application crate의 포트와 유스케이스(질문, 룰북, 세션, STT, 오답노트, 키 검증) 추가`.

### Task 4: FeedbackUseCase

**Files:**
- Create: `backend/crates/application/src/feedback/{usecase.rs, policy.rs}` (필요하면 `usecase/` 폴더로 나눈다)
- Java 기준: `application/feedback/usecase/FeedbackUseCase.java`(983줄), `application/feedback/policy/FeedbackModelPolicy.java`
- Java 테스트: `FeedbackUseCaseTest`(570줄), `FeedbackModelPolicyTest`

**Interfaces:**
- Consumes: Task 3의 `LlmClient`, `RulebookUseCase`, `WrongNoteUseCase`, `MetricsPort`, `FeedbackPolicy`, `test_support`.
- Produces: `FeedbackUseCase`(Java와 같은 공개 메서드), `FeedbackModelPolicy`. 프롬프트 문자열은 Java와 한 글자도 다르지 않게 옮긴다.

- [ ] **Step 1:** 두 테스트 파일을 옮긴다. 실패 확인.
- [ ] **Step 2:** 구현을 옮긴다. 프롬프트와 JSON 스키마 문자열은 Java 소스에서 그대로 복사한다.
- [ ] **Step 3:** 테스트, clippy, fmt 통과.
- [ ] **Step 4:** 커밋 `feat: FeedbackUseCase와 피드백 모델 정책 추가`.

### Task 5: DataTransferUseCase와 0.2.x 백업 호환

**Files:**
- Create: `backend/crates/application/src/datatransfer/{mod.rs,usecase.rs,model.rs}`
- Create: `backend/crates/application/tests/fixtures/backup-0.2.x-iso.zip`, `backup-0.2.x-epoch.zip`(테스트 안에서 `zip` crate로 만들어도 된다)
- Java 기준: `application/datatransfer/**`, 테스트 `DataTransferUseCaseTest`

**Interfaces:**
- Consumes: Task 3의 포트와 `test_support`.
- Produces: `DataTransferUseCase::export_zip(&self) -> AppResult<Vec<u8>>`, `import_zip(&self, bytes: &[u8]) -> AppResult<DataTransferImportResult>`. `DataTransferImportResult`의 필드는 Java와 같다.

- [ ] **Step 1:** `DataTransferUseCaseTest`를 옮긴다.
- [ ] **Step 2:** Review Focus 1의 테스트를 추가한다:
  - `import_accepts_iso_timestamps`: `createdAt`이 `"2026-02-19T00:00:00Z"`인 zip을 가져오면 같은 시각으로 저장된다.
  - `import_accepts_epoch_second_timestamps`: `createdAt`이 `1771459200.000000000`(숫자)인 zip을 가져오면 `2026-02-19T00:00:00Z`로 저장된다.
  - `import_ignores_unknown_fields`: 각 JSON에 `"extra": 1`이 있어도 성공한다.
  - `import_fills_missing_optional_fields`: 룰북의 `questionGroup`, 질문 그룹의 `tags`가 없어도 성공하고 각각 `None`, 빈 목록이 된다.
  - `export_then_import_round_trips_korean_rulebook_filename`: 파일 이름이 `오픽 규칙.md`인 룰북을 내보내고 빈 저장소에 가져오면 이름과 내용이 같다.
- [ ] **Step 3:** 실행해서 실패 확인.
- [ ] **Step 4:** 구현을 옮긴다. zip 안의 경로 이름과 JSON 필드 이름은 Java의 스냅샷 클래스와 같다.
- [ ] **Step 5:** 테스트, clippy, fmt 통과.
- [ ] **Step 6:** 커밋 `feat: 백업 내보내기/가져오기 유스케이스 추가(0.2.x zip 호환)`.

### Task 6: 음성 세션

**Files:**
- Create: `backend/crates/application/src/voice/{mod.rs, usecase.rs, facade/…}`
- Java 기준: `application/voice/usecase/{VoiceSessionFacade,VoiceSessionUseCase}.java`
- Java 테스트: application 아래 `voice` 패키지의 테스트 전부(`VoiceSessionUseCaseTest`, `VoiceSessionUseCaseValidationTest` 등)

**Interfaces:**
- Consumes: Task 3~4의 유스케이스와 포트, `AsyncExecutor`, `VoiceEventSink`.
- Produces: `VoiceSessionUseCase`의 공개 메서드(Java와 같은 이름: 세션 열기, 이벤트 싱크 등록/해제, 복구 조회, 오디오 청크 추가, 답변 제출, 중지). SSE 이벤트 이름과 payload의 JSON 필드는 Java와 같다. payload는 `serde_json::Value`로 만든다.
- 동시성: Java의 `synchronized (audioLock)`, `synchronized (eventLock)`은 세션 컨텍스트 안의 `Mutex` 두 개로 옮긴다. 잠금을 쥔 채 `LlmClient`, `SttGateway`, `TtsGateway`를 부르지 않는다(Java도 그렇다).
- `facade`는 책임별 파일로 나눈다(세션 컨텍스트, 턴 처리, 이벤트 발행, 프리페치). 밖에서 보이는 동작만 같으면 된다.

- [ ] **Step 1:** voice 테스트를 옮긴다. `ImmediateExecutor`와 `RecordingSink`를 쓴다. 실패 확인.
- [ ] **Step 2:** 구현을 옮긴다.
- [ ] **Step 3:** 테스트 추가 `sink_send_false_stops_emitting_and_keeps_session_recoverable`: 싱크가 `false`를 돌려준 뒤에도 패닉이 없고, 복구 조회가 마지막 상태를 돌려준다.
- [ ] **Step 4:** 테스트, clippy, fmt 통과.
- [ ] **Step 5:** 커밋 `feat: 음성 세션 유스케이스 추가`.

### Task 7: infrastructure 설정, 지표, 파일 저장, 세션 저장

**Files:**
- Create: `backend/crates/infrastructure/{Cargo.toml,src/lib.rs,src/config.rs}`, `src/metrics/mod.rs`, `src/storage/{mod.rs,rulebook_files.rs,data_dir.rs}`, `src/session/mod.rs`, `src/executor.rs`
- Java 기준: `infrastructure/shared/config/AppProperties.java`, `infrastructure/shared/metrics/MicrometerMetricsAdapter.java`, `infrastructure/rulebook/storage/FileSystemRulebookStore.java`, `infrastructure/session/store/InMemorySessionStore.java`, `application.yml`
- Java 테스트: `AppPropertiesTest`, `MicrometerMetricsAdapterTest`, `FileSystemRulebookStoreTest`, `InMemorySessionStoreTest`

**Interfaces:**
- Produces:
  - `AppConfig`(= `AppProperties`): `AppConfig::default()`는 `application.yml`의 기본값, `AppConfig::from_env_with(lookup: impl Fn(&str) -> Option<String>) -> AppResult<AppConfig>`와 `AppConfig::from_env()`. `data_dir` 기본값은 `<홈>/OMyPIc`. 추가 필드 `port: u16`(환경 변수 `OMYPIC_PORT`, 기본 4317). `AppConfig`가 `FeedbackPolicy`, `RagPolicy`, `SttPolicy`, `VoicePolicy`, `DataDirProvider`를 구현한다(Java와 같다).
  - `InMemoryMetrics: MetricsPort`. 추가 메서드 `names() -> Vec<String>`, `snapshot(name: &str) -> Option<MetricSnapshot>`. 지표 이름은 Java와 같다(`stt.latency`, `stt.requests`, `stt.errors` 등 `MicrometerMetricsAdapter`가 등록하는 이름 전부). `MetricSnapshot`은 카운터면 `COUNT`, 타이머면 `COUNT`, `TOTAL_TIME`(초), `MAX`(초)를 가진다.
  - `FileSystemRulebookStore: RulebookFileStore`, `InMemorySessionStore: SessionStorePort`, `ThreadPoolExecutor: AsyncExecutor`(작업마다 `std::thread::spawn`).

- [ ] **Step 1:** 위 Java 테스트 4개를 옮긴다(`AppPropertiesTest`는 `from_env_with`에 맵을 넘겨 검사한다). 실패 확인.
- [ ] **Step 2:** 구현한다.
- [ ] **Step 3:** 테스트 추가 `rulebook_store_keeps_korean_filename_bytes`: `오픽 규칙.md`를 저장하고 읽으면 내용이 같다.
- [ ] **Step 4:** 테스트, clippy, fmt 통과.
- [ ] **Step 5:** 커밋 `feat: infrastructure crate의 설정, 지표, 파일 저장 추가`.

### Task 8: SQLite 영속성

**Files:**
- Create: `backend/crates/infrastructure/src/sqlite/{mod.rs,schema.rs,question.rs,rulebook.rs,wrongnote.rs}`
- Java 기준: `infrastructure/{question,rulebook,wrongnote}/persistence/mongo/**`
- Java 테스트: `QuestionListMongoAdapterTest`, `RulebookMongoAdapterTest`, `WrongNoteMongoAdapterTest`, `WrongNoteRecentQueueMongoAdapterTest`

**Interfaces:**
- Consumes: Task 3의 포트 trait.
- Produces:
  - `Database`: `Database::open(path: &Path) -> AppResult<Database>`, `Database::open_in_memory()`. 내부는 `Arc<Mutex<rusqlite::Connection>>`. 열 때 `PRAGMA journal_mode=WAL`, `PRAGMA foreign_keys=ON`, `busy_timeout` 5초를 설정하고 `schema::migrate`를 실행한다. `Clone` 가능.
  - `schema::migrate(conn)`: `PRAGMA user_version`이 0이면 버전 1 스키마를 만들고 1로 올린다. 이미 1이면 아무것도 하지 않는다. 더 큰 값이면 `AppError::InvalidState("Unsupported database version: N")`.
  - 버전 1 테이블: `question_groups(id TEXT PRIMARY KEY, name, tags_json, created_at, updated_at)`, `question_items(id TEXT PRIMARY KEY, group_id REFERENCES question_groups ON DELETE CASCADE, position INTEGER, …QuestionItem 필드)`, `rulebooks(id TEXT PRIMARY KEY, filename, path, scope, question_group, enabled INTEGER, created_at, updated_at)`, `wrong_notes(id TEXT PRIMARY KEY, pattern UNIQUE, count INTEGER, short_summary, last_seen_at)`, `wrong_note_queue_states(id TEXT PRIMARY KEY, patterns_json, updated_at)`, `rulebook_chunks(id INTEGER PRIMARY KEY, rulebook_id, filename, position INTEGER, text, embedding BLOB)`, `rulebook_index_meta(key TEXT PRIMARY KEY, value TEXT)`. 시각은 ISO-8601 TEXT. `wrong_notes.pattern`에 UNIQUE를 걸지는 Mongo 어댑터의 동작을 보고 정한다(중복 pattern을 허용하면 걸지 않는다).
  - `SqliteQuestionGroupAdapter: QuestionGroupPort`, `SqliteRulebookAdapter: RulebookPort`, `SqliteWrongNoteAdapter: WrongNotePort`, `SqliteWrongNoteRecentQueueAdapter: WrongNoteRecentQueuePort`. 각각 `new(db: Database)`.

- [ ] **Step 1:** 위 Java 테스트 4개를 `Database::open_in_memory()` 기준으로 옮긴다. Mongo 저장소 mock을 검사하던 테스트는 "저장한 뒤 다시 읽으면 같은 값"을 검사하는 테스트로 바꾼다. 정렬 순서(예: 오답노트의 정렬 기준)는 Mongo 어댑터와 같게 단언한다.
- [ ] **Step 2:** 테스트 추가:
  - `migrate_is_idempotent`: 같은 파일을 두 번 열어도 데이터가 남고 `user_version`이 1이다.
  - `open_rejects_newer_schema`: `user_version`이 99인 파일을 열면 `InvalidState`다.
  - `question_group_keeps_item_order`: 질문 5개를 저장하고 읽으면 순서가 같다.
  - `concurrent_writers_and_readers_do_not_fail`: 스레드 8개가 각각 오답노트 50개를 쓰고 다른 스레드 4개가 질문 목록을 200번 읽어도 오류가 0건이다(파일 DB, tempfile).
  - `rulebook_keeps_korean_filename`: `오픽 규칙.md` 저장 후 조회.
- [ ] **Step 3:** 실행, 실패 확인.
- [ ] **Step 4:** 구현한다.
- [ ] **Step 5:** 테스트, clippy, fmt 통과.
- [ ] **Step 6:** 커밋 `feat: SQLite 영속성 어댑터 추가(MongoDB 대체)`.

### Task 9: 임베딩과 룰북 인덱스

**Files:**
- Create: `backend/crates/infrastructure/src/embedding/{mod.rs,fasttext.rs,hash.rs}`, `src/sqlite/rulebook_index.rs`
- Java 기준: `infrastructure/rulebook/rag/**`
- Java 테스트: `FastTextEmbeddingAdapterTest`, `LocalHashEmbeddingAdapterTest`, `LuceneRulebookIndexAdapterTest`

**Interfaces:**
- Consumes: `AppConfig`(rag 설정), `Database`, `EmbeddingPort`, `RulebookIndexPort`.
- Produces: `FastTextEmbeddingAdapter: EmbeddingPort`, `LocalHashEmbeddingAdapter: EmbeddingPort`, `SqliteRulebookIndexAdapter: RulebookIndexPort`.
- fastText: 모델 파일 탐색, sha256 검증, 내려받기와 재시도, `.vec`/`.vec.gz` 읽기, `model-max-vocab` 제한, 토큰화, 평균과 정규화, 캐시, `allow-hash-fallback` 동작을 Java와 같게 한다. 내려받기는 `reqwest::blocking`으로 한다.
- 해시 임베딩: Java의 해시 함수와 같은 벡터가 나와야 한다(Java `String.hashCode()`를 쓰면 같은 계산을 직접 구현한다).
- 인덱스: `rulebook_chunks`에 청크와 임베딩(리틀엔디언 f32 BLOB)을 저장한다. 검색은 활성 룰북 id의 청크 전부와 질의 벡터의 **유클리드 거리**를 계산해 가까운 순으로 `top_k`개를 돌려준다(Lucene `KnnVectorField`의 기본 유사도와 같다). `rulebook_index_meta`의 provider, model version, dimension이 현재 임베딩과 다르면 청크를 모두 지우고 메타를 갱신한다(Java `ensureIndexCompatibility`와 같다).

- [ ] **Step 1:** Java 테스트 3개를 옮긴다. fastText 테스트의 가짜 모델 파일과 내려받기 서버는 tempfile과 httpmock으로 만든다. 실패 확인.
- [ ] **Step 2:** 테스트 추가 `search_ranks_by_euclidean_distance`: 3차원 가짜 임베딩으로 청크 4개를 넣고 질의에 가장 가까운 2개가 거리 순으로 나온다. `search_skips_disabled_rulebooks`: 활성 id에 없는 룰북의 청크는 나오지 않는다.
- [ ] **Step 3:** 구현한다.
- [ ] **Step 4:** 테스트, clippy, fmt 통과.
- [ ] **Step 5:** 커밋 `feat: fastText/해시 임베딩과 SQLite 룰북 인덱스 추가(Lucene 대체)`.

### Task 10: OpenAI 연동

**Files:**
- Create: `backend/crates/infrastructure/src/openai/{mod.rs,http.rs,llm_client.rs,parameter_policy.rs,log_formatter.rs,stt_gateway.rs,tts_gateway.rs,key_probe.rs}`, `src/llm/{feedback_json_parser.rs,prompt_context_renderer.rs}`
- Java 기준: `infrastructure/feedback/llm/{OpenAi*,Gpt5ModelParameterPolicy,LegacyChatModelParameterPolicy,FeedbackJsonParser,LlmPromptContextRenderer}.java`, `infrastructure/stt/gateway/OpenAiSttGateway.java`, `infrastructure/voice/gateway/OpenAiTtsGateway.java`, `infrastructure/apikey/probe/WebClientApiKeyProbeAdapter.java`, `infrastructure/shared/config/WebClientConfig.java`
- Java 테스트: `OpenAiLlmClientTest`, `OpenAiModelParameterPolicyResolverTest`, `OpenAiSttGatewayTest`, `WebClientApiKeyProbeAdapterTest`, `WebClientConfigTest`

**Interfaces:**
- Consumes: `AppConfig`(integrations.openai), 포트 `LlmClient`, `SttGateway`, `TtsGateway`, `ApiKeyProbePort`.
- Produces: `OpenAiLlmClient: LlmClient`, `OpenAiSttGateway: SttGateway`, `OpenAiTtsGateway: TtsGateway`, `HttpApiKeyProbeAdapter: ApiKeyProbePort`(OpenAI와 Anthropic 키 확인을 모두 맡는다. Java 어댑터와 같다), `FeedbackJsonParser`, `LlmPromptContextRenderer`. HTTP 클라이언트는 `reqwest::blocking::Client` 하나를 공유한다. 타임아웃 값은 `WebClientConfig`와 같다.
- 요청 경로(`/v1/responses`, `/v1/chat/completions`, STT, TTS), 헤더, 본문 JSON은 Java가 보내는 것과 같아야 한다.

- [ ] **Step 1:** Java 테스트를 옮긴다. MockWebServer는 httpmock으로 바꾸고, 요청 본문 JSON과 헤더를 같은 값으로 단언한다. 실패 확인.
- [ ] **Step 2:** 구현한다.
- [ ] **Step 3:** 테스트, clippy, fmt 통과.
- [ ] **Step 4:** 커밋 `feat: OpenAI LLM/STT/TTS 연동과 키 확인 추가`.

### Task 11: Anthropic 연동

**Files:**
- Create: `backend/crates/infrastructure/src/anthropic/{mod.rs,llm_client.rs}`
- Java 기준: `infrastructure/feedback/llm/AnthropicLlmClient.java`, `infrastructure/shared/anthropic/AnthropicClientFactory.java`
- Java 테스트: `AnthropicLlmClientTest`

**Interfaces:**
- Consumes: `AppConfig`(integrations.anthropic), `LlmClient`, `FeedbackJsonParser`, `LlmPromptContextRenderer`.
- Produces: `AnthropicLlmClient: LlmClient`. Java SDK가 보내던 것과 같은 요청을 `reqwest::blocking`으로 만든다: `POST {base-url}/v1/messages`, 헤더 `x-api-key`, `anthropic-version: 2023-06-01`, 필요한 `anthropic-beta` 값, 본문의 `model`, `max_tokens`, `system`, `messages`, `output_config`(effort와 JSON 스키마 형식), 해당 모델에서의 `fallbacks`. 정확한 필드 이름과 beta 헤더 값은 `AnthropicLlmClientTest`가 단언하는 요청 본문과 SDK 3.3.0의 직렬화 결과를 따른다. 재시도 횟수(`max-retries`)와 타임아웃(`timeout-seconds`)을 지킨다. 재시도 대상은 SDK와 같다(연결 오류, 408, 409, 429, 5xx).

- [ ] **Step 1:** `AnthropicLlmClientTest`를 옮긴다. 실패 확인.
- [ ] **Step 2:** 테스트 추가 `retries_on_529_then_succeeds`: 첫 응답 529, 둘째 응답 200이면 결과를 돌려주고 요청이 2번 갔다. `gives_up_after_max_retries`: 계속 500이면 `max-retries + 1`번 요청한 뒤 오류다.
- [ ] **Step 3:** 구현한다.
- [ ] **Step 4:** 테스트, clippy, fmt 통과.
- [ ] **Step 5:** 커밋 `feat: Anthropic Messages API 연동 추가`.

### Task 12: presentation (axum)

**Files:**
- Create: `backend/crates/presentation/{Cargo.toml,src/lib.rs,src/error.rs,src/state.rs,src/sse.rs}`, `src/routes/{apikey,datatransfer,meta,question,rulebook,mode,voice,wrongnote,actuator}.rs`
- Java 기준: `presentation/**`
- Java 테스트: `presentation/**/*ControllerTest`, `WebConfigTest`

**Interfaces:**
- Consumes: application의 유스케이스 전부, `InMemoryMetrics`(지표 조회용 trait `MetricsReader { names(), snapshot(name) }`를 presentation에 정의하고 infrastructure가 구현한다).
- Produces:
  - `AppState`: 유스케이스들의 `Arc`와 `Arc<dyn MetricsReader>`를 담는다. `Clone`.
  - `pub fn router(state: AppState) -> axum::Router`.
  - `impl IntoResponse for ApiError`: `InvalidArgument`/`InvalidState` → 400 `{"message"}`, `NotFound` → 404, `InvalidGroupTags` → 400(위 형식), 요청 JSON 파싱 실패와 검증 실패 → 400, 그 밖 → 500 `{"message":"Internal server error"}`.
  - CORS: 모든 origin, 메서드, 헤더 허용(Java `WebConfig`와 같다).
  - 본문 제한: JSON 64MB, multipart 64MB.
  - 핸들러는 유스케이스를 `tokio::task::spawn_blocking`으로 부른다.
  - SSE(`GET /api/voice/sessions/{id}/events`): `tokio::sync::mpsc::unbounded_channel`을 만들고 `VoiceEventSink` 구현이 `tx.send(...).is_ok()`를 돌려준다. 스트림이 끊기면(수신 쪽 drop) 싱크 등록을 해제한다. Java가 keep-alive나 초기 이벤트를 보내면 같은 것을 보낸다.
  - `/actuator/health` → `{"status":"UP"}`. `/actuator/metrics` → `{"names":[…]}`. `/actuator/metrics/{name}` → `{"name":…, "measurements":[{"statistic":"COUNT","value":…}, …]}`, 없는 이름은 404.

- [ ] **Step 1:** Java 컨트롤러 테스트를 옮긴다. `tower::ServiceExt::oneshot`으로 `router`에 직접 요청하고, 유스케이스는 `test_support` 가짜 포트로 조립한다. 상태 코드와 응답 JSON을 같은 값으로 단언한다. 실패 확인.
- [ ] **Step 2:** 테스트 추가:
  - `audio_chunk_accepts_8mb_json_body`: 8MB base64 청크를 보내면 413이 아니다.
  - `rulebook_upload_accepts_korean_filename`: multipart 파일 이름 `오픽 규칙.md`로 올리면 응답의 `filename`이 같다.
  - `sse_stream_delivers_events_in_order`: 싱크에 보낸 이벤트 3개가 `event:`/`data:` 줄로 순서대로 나온다.
  - `sse_disconnect_unregisters_sink`: 응답 본문을 drop하면 그 뒤 `send`가 `false`다.
  - `unknown_route_returns_404`, `malformed_json_returns_400_with_message`.
  - `metrics_endpoint_reports_counter`: STT 요청을 2번 기록하면 `/actuator/metrics/stt.requests`의 `COUNT`가 2다.
- [ ] **Step 3:** 구현한다.
- [ ] **Step 4:** 테스트, clippy, fmt 통과.
- [ ] **Step 5:** 커밋 `feat: axum HTTP API와 SSE 추가`.

### Task 13: server 조립, 실제 실행 확인, Java 제거

**Files:**
- Create: `backend/crates/server/{Cargo.toml,src/lib.rs,src/main.rs,src/wiring.rs}`
- Modify: `README.md`(개발 실행 방법), `.github/workflows/release.yml`(test 작업의 백엔드 테스트를 `cargo test --workspace`로), `.gitignore`
- Delete: `backend/{domain,application,infrastructure,presentation,bootstrap,gradle,scripts}`, `backend/{build.gradle,settings.gradle,gradlew,gradlew.bat}` 등 Gradle 파일 전부(Step 6에서)
- Java 기준: `bootstrap/**`

**Interfaces:**
- Consumes: 앞의 모든 crate.
- Produces (계획 2의 Tauri가 쓴다):
  - `omypic_server::RunningServer { pub addr: std::net::SocketAddr, … }`, `RunningServer::shutdown(self)`(진행 중인 요청을 기다린 뒤 종료).
  - `pub async fn start(config: AppConfig, bind: std::net::SocketAddr) -> anyhow::Result<RunningServer>`: 데이터 폴더를 만들고, `<data_dir>/omypic.db`를 열고, 어댑터와 유스케이스를 조립하고(`UseCaseConfig.java`와 같은 조합. rag provider 설정에 따라 fastText 또는 해시 임베딩), `bind`에 리슨한다. `bind`의 포트가 0이면 OS가 고른 포트가 `addr`에 들어간다.
  - 실행 파일 `omypic-server`: `AppConfig::from_env()`로 읽고 `127.0.0.1:<port>`에 띄운다. Ctrl+C로 정상 종료한다. 포트가 이미 쓰이고 있으면 `Failed to bind 127.0.0.1:4317: …`를 출력하고 종료 코드 1로 끝난다.

- [ ] **Step 1:** 테스트 `server_starts_on_ephemeral_port_and_answers_ping`: 임시 데이터 폴더와 포트 0으로 `start`를 부르고 `GET /api/ping`과 `/actuator/health`가 200이다. `question_group_crud_round_trip_over_http`: 그룹 생성, 조회, 질문 추가, 삭제를 HTTP로 수행한다. `data_survives_restart`: 그룹을 만들고 서버를 끄고 같은 폴더로 다시 켜면 그룹이 있다. `export_import_over_http_round_trips`: `/api/data-transfer/export`의 zip을 새 데이터 폴더의 서버에 `/import`로 넣으면 그룹과 룰북이 같다(rag provider는 해시).
- [ ] **Step 2:** 실행, 실패 확인. 구현. 통과 확인.
- [ ] **Step 3:** `cargo test --workspace`, clippy, fmt 통과. 커밋 `feat: Rust 서버 실행 파일과 조립 추가`.
- [ ] **Step 4:** 실제 실행 확인. `OMYPIC_RAG_PROVIDER=hash OMYPIC_RAG_ALLOW_HASH_FALLBACK=true cargo run -p omypic-server`로 띄우고, `frontend`에서 `npm ci && npx vite`를 띄운 뒤 Playwright(Chromium)로 `http://localhost:5173`을 연다. 다음을 직접 조작하고 화면을 캡처한다: 질문 그룹 만들기와 질문 추가, 룰북(.md) 올리기와 켜고 끄기, 백업 내보내기와 가져오기, 오답노트 화면 열기, AI OFF로 세션 시작해서 첫 질문이 화면에 뜨는 것. 브라우저 콘솔과 서버 로그에 오류가 없어야 한다. 발견한 차이는 고치고 테스트를 추가한다.
- [ ] **Step 5:** `npm test`(frontend)가 통과하는지 확인한다.
- [ ] **Step 6:** Java 소스와 Gradle 파일을 삭제한다. README의 백엔드 실행 방법을 `cargo run -p omypic-server`로, MongoDB 관련 설명을 SQLite로 고친다. `release.yml`의 test 작업에서 `setup-java`와 `./gradlew test`를 `cargo test --workspace`로 바꾼다(build 작업은 계획 2에서 바꾼다. 그때까지 build 작업은 깨진 상태이므로 계획 2가 끝나기 전에는 릴리스를 만들지 않는다).
- [ ] **Step 7:** 커밋 `refactor: Java 백엔드 제거, 문서와 CI를 Rust 기준으로 변경`. 브랜치에 push.
