# OMyPIc Rust 전환 설계

- 작성일: 2026-11-19
- 상태: 승인 대기
- 대상 버전: 0.3.0 (현재 0.2.1)

## 1. 의도

OMyPIc 데스크톱 앱에서 JVM, MongoDB, Electron을 걷어내고 Rust로 옮긴다.

지금 설치본은 Electron 안에 Spring Boot JAR, JRE 17, MongoDB 바이너리를 함께 넣는다. 앱을 켜면 mongod와 JVM 프로세스를 차례로 띄우고 준비될 때까지 기다린다. 전환 뒤에는 Tauri 앱 하나가 백엔드를 같은 프로세스 안에서 실행하고, 데이터는 SQLite 파일 하나에 저장한다.

사용자가 정한 것:

- Rust 전환을 3단계(백엔드 포팅, MongoDB를 SQLite로 교체, Electron을 Tauri로 교체)까지 모두 진행한다.
- 운영체제 내장 음성(`device-voice`)은 없앤다. 무료 TTS는 Kokoro 같은 모델 음성으로 한다.
- Kokoro가 읽지 못하는 한국어 문장을 위해 한국어 로컬 TTS 모델을 추가한다.
- Kokoro와 Whisper는 지금처럼 화면(웹뷰) 쪽에서 실행한다.

전환 이유는 사용자가 따로 밝히지 않았다. 이 문서는 설치본 크기와 실행 구조 단순화를 목표로 가정한다(작성자의 판단).

## 2. 목표, 비목표, 실패 조건

### 목표

1. `backend/`의 Java 코드(151개 파일, 약 12,300줄)를 Rust로 옮긴다. HTTP API의 경로, 요청/응답 JSON, SSE 이벤트 이름과 내용은 그대로 둔다.
2. 저장소를 MongoDB에서 SQLite로 바꾼다. Lucene 인덱스도 SQLite로 대체한다.
3. Electron을 Tauri v2로 바꾼다. 백엔드는 Tauri 프로세스 안에서 실행한다.
4. `device-voice`를 제거하고 한국어 로컬 TTS 모델을 추가한다.
5. 기존 테스트가 검증하던 동작을 Rust 테스트로 옮긴다(Java 테스트 47개 파일의 케이스). 프런트엔드 vitest는 계속 통과해야 한다.
6. GitHub Actions 릴리스 워크플로가 Windows(NSIS, x64)와 macOS(DMG, Apple Silicon) 설치본을 만든다.

### 비목표

- 새 기능 추가, 화면 디자인 변경, 프롬프트 내용 변경.
- STT/TTS를 Rust 네이티브로 실행하는 것. 로컬 모델은 계속 웹뷰의 Web Worker에서 돈다.
- Linux 설치본 배포. Linux 빌드는 이 저장소의 개발과 자동 검증에만 쓴다.
- 코드 서명과 자동 업데이트. 지금처럼 서명 없는 설치본을 만든다.
- 기존 MongoDB 데이터 파일을 새 앱이 직접 읽는 것(아래 5.4 참고).

### 실패 조건 (동작하더라도 실패로 보는 것)

- 프런트엔드가 기대하는 API 응답이나 SSE 이벤트가 달라져 화면 동작이 바뀌는 것.
- 0.2.x에서 내보낸 백업 zip을 0.3.0이 가져오지 못하는 것.
- 설치본에 JRE, MongoDB, Node 런타임 중 하나라도 남는 것.
- 한국어 문장을 영어 음성이 읽거나, 영어 문장을 한국어 음성이 읽는 것.

## 3. 제약

- 이 작업 환경(Linux 컨테이너)에서는 Windows와 macOS 설치본을 실행할 수 없다. 두 OS에서의 실제 동작(마이크 권한, 모델 내려받기, 음성 재생)은 CI가 만든 설치본을 사용자가 직접 실행해 확인해야 한다.
- 이 환경에서는 `huggingface.co`와 `dl.fbaipublicfiles.com`에 접속할 수 없다. 로컬 음성 모델과 fastText 모델을 실제로 내려받아 검증하려면 네트워크 허용이 필요하다. 허용되지 않으면 해당 부분은 가짜 서버를 쓴 자동 테스트까지만 검증한다.
- OpenAI와 Anthropic API 키가 없다. 두 API 연동은 지금 Java 테스트와 같은 방식(가짜 HTTP 서버)으로 검증한다.
- 작업 브랜치는 `claude/lucid-ride-po0c5o` 하나다.

## 4. 검토한 접근

### A. 백엔드와 저장소를 한 번에 옮기고, 그다음 Tauri (선택)

Rust로 옮기면서 저장소를 바로 SQLite로 만든다. 그 뒤에 Tauri로 감싼다. 음성 변경은 마지막에 한다.

- 장점: MongoDB용 Rust 코드를 썼다가 버리지 않는다. 이 환경에는 mongod가 없어서 MongoDB 구현은 어차피 검증할 수 없다.
- 단점: 원래 말한 1단계와 2단계가 합쳐져 첫 덩어리가 커진다.

### B. 원래 순서대로 세 단계를 따로 진행

Rust + MongoDB로 먼저 옮기고, SQLite로 다시 바꾸고, Tauri로 간다.

- 장점: 단계마다 바뀌는 것이 적다.
- 단점: 영속성 계층을 두 번 쓴다. 중간 단계를 이 환경에서 실행해 볼 수 없다.

### C. 백엔드를 별도 실행 파일(사이드카)로 두고 Tauri가 띄우기

- 장점: 지금 Electron 구조와 가장 비슷하다.
- 단점: 프로세스 시작과 종료, 준비 대기 코드가 그대로 남는다. 한 프로세스로 줄인다는 이득을 놓친다.

**선택: A.** 사용자는 3단계까지 모두 진행하라고 했고 방법은 맡겼다. 1단계와 2단계를 합치는 것, 백엔드를 Tauri 프로세스 안에서 돌리는 것은 작성자의 판단이다.

## 5. 설계

### 5.1 저장소 구조

```
Cargo.toml                    # workspace
backend/
  crates/
    domain/                   # 순수 도메인. 외부 의존 없음
    application/              # 유스케이스와 포트(trait)
    infrastructure/           # SQLite, 파일, OpenAI/Anthropic HTTP, 임베딩
    presentation/             # axum 라우터, DTO, 오류 변환
    server/                   # 실행 파일. 설정 읽기와 조립
frontend/
  src/                        # React (거의 그대로)
  src-tauri/                  # Tauri 앱. backend crate를 의존성으로 사용
```

Java의 `domain / application / infrastructure / presentation / bootstrap` 모듈을 crate로 1:1 대응시킨다. 의존 방향도 같다. Java 소스와 Gradle 파일은 Rust 쪽이 같은 테스트를 통과한 뒤 삭제한다.

### 5.2 백엔드 구성 요소

| 지금 | 전환 뒤 |
| --- | --- |
| Spring Web, Validation | `axum`, 직접 작성한 검증 |
| SSE (`SseEmitter`) | `axum::response::sse` |
| WebClient, Spring AI | `reqwest`로 OpenAI `/v1/responses`, `/v1/chat/completions`, STT, TTS 직접 호출 |
| `anthropic-java` | `reqwest`로 Messages API 직접 호출. 지금 쓰는 structured outputs, effort, 서버 측 fallback 요청 본문을 그대로 재현 |
| Jackson | `serde` |
| Spring Data MongoDB | `rusqlite`(SQLite 내장 빌드) |
| Lucene `KnnVectorField` | SQLite에 청크와 임베딩을 저장하고 전수 비교로 검색 |
| Micrometer, Actuator | 5.5 참고 |
| `application.yml`, `AppProperties` | 같은 이름의 환경 변수와 같은 기본값을 읽는 설정 구조체 |

`OMYPIC_*` 환경 변수의 이름과 기본값은 유지한다. `OMYPIC_MONGODB_URI`만 없어진다.

### 5.3 HTTP API

경로, 메서드, 요청/응답 JSON, 상태 코드, 오류 형식(`{"message": "..."}`)을 그대로 둔다.

- `/api/ping`, `/api/meta/models`, `/api/keys/verify`, `/api/modes`
- `/api/question-groups` 이하, `/api/rulebooks` 이하, `/api/wrongnotes`
- `/api/data-transfer/export`, `/api/data-transfer/import`
- `/api/voice/sessions` 이하 (SSE 이벤트: `session.ready`, `question.prompt`, `stt.final`, `stt.skipped`, `answer.captured`, `feedback.final`, `tts.audio`, `tts.text`, `tts.error`, `session.stopped`, `session.error`)
- `/actuator/health`

바뀌는 것:

- 서버는 `127.0.0.1`에만 바인딩한다. 단독 실행(`cargo run`)의 기본 포트는 지금처럼 4317이다. Tauri 앱 안에서는 빈 포트를 자동으로 골라 쓰고 화면에 그 주소를 알려 준다. 포트 충돌로 앱이 안 켜지는 일을 막기 위해서다.
- `/api/meta/models`의 TTS 목록에서 `device-voice`가 빠진다(5.7).

### 5.4 데이터 저장

SQLite 파일 하나(`<데이터 폴더>/omypic.db`)에 저장한다.

| MongoDB 컬렉션 | SQLite |
| --- | --- |
| `question_groups` (질문은 내장 배열) | `question_groups`, `question_items` |
| `rulebooks` | `rulebooks` |
| `wrong_notes` | `wrong_notes` |
| `wrong_note_queue_states` | `wrong_note_queue_states` |
| Lucene 인덱스 폴더와 메타 파일 | `rulebook_chunks`(본문, 임베딩 BLOB), `rulebook_index_meta` |

- 스키마 버전은 `PRAGMA user_version`으로 관리한다.
- 룰북 원본 파일은 지금처럼 데이터 폴더에 파일로 둔다.
- 룰북 검색은 활성화된 룰북의 청크 전체와 질의 벡터를 직접 비교해 상위 N개를 고른다. 유사도 함수는 지금 Lucene이 쓰는 것(유클리드 거리)과 같게 한다. 사용자가 올리는 룰북은 양이 적어서 근사 검색 인덱스가 필요 없고, 전수 비교는 근사 검색보다 결과가 정확하다. 임베딩 제공자, 모델 버전, 차원이 바뀌면 인덱스를 비우는 지금 동작도 유지한다.
- 음성 세션 상태는 지금처럼 메모리에만 둔다.

**기존 사용자 데이터.** 새 앱은 MongoDB 데이터 파일을 읽지 못한다(mongod 없이는 읽을 수 없다). 이전 경로는 이미 있는 백업 기능이다. 0.2.x에서 설정 화면의 내보내기로 zip을 만들고, 0.3.0에서 가져온다. 그래서 백업 zip 형식(`schemaVersion` 1, 파일 경로, JSON 필드 이름)은 그대로 유지하고, 0.2.x가 만든 zip을 테스트 자료로 넣어 가져오기를 검증한다. README와 릴리스 노트에 "업데이트 전에 내보내기"를 적는다. 내보내지 않고 업데이트한 사용자는 빈 앱을 보게 된다. MongoDB 데이터 파일은 원래 폴더(`<사용자 앱 데이터>/omypic-frontend/db`)에 그대로 남으므로 0.2.x를 다시 설치하면 내보낼 수 있다. 앱 안에서 따로 알리는 화면은 만들지 않는다. (작성자의 판단)

### 5.5 지표

`/actuator/health`는 `{"status":"UP"}`을 돌려준다. 문서(`docs/005 버그 수정/AC_METRIC_CHECK.md`)가 쓰는 `/actuator/metrics`와 `/actuator/metrics/{name}`은 지금 기록하는 지표(STT, 피드백, TTS, 질문, 음성 턴, 룰북 업로드의 횟수·오류·지연)에 한해 같은 JSON 모양으로 제공한다. Micrometer가 자동으로 내던 JVM 지표는 제공하지 않는다. (작성자의 판단)

### 5.6 데스크톱 앱 (Tauri v2)

- Tauri 프로세스가 시작할 때 같은 프로세스 안에서 axum 서버를 띄운다. 별도 프로세스, 포트 대기, 종료 처리가 없어진다.
- 화면은 지금처럼 `fetch`와 `EventSource`로 `http://127.0.0.1:<포트>`를 호출한다. 그래서 브라우저에서 `vite`만 띄우고 `cargo run` 백엔드에 붙는 개발 방식도 그대로 된다.
- `window.omypic` 인터페이스(`getBackendUrl`, `getApiKey`, `setApiKey`, `saveBackupFile`, `pickBackupFile`)는 이름과 동작을 유지한다. Electron preload 대신 Tauri 명령을 부르는 작은 연결 모듈이 이 객체를 만든다. 화면 코드의 나머지는 건드리지 않는다.
- API 키는 `keyring` crate로 운영체제 보관소(Windows 자격 증명 관리자, macOS 키체인)에 저장한다. 서비스 이름 `OMyPIc`와 계정 이름(`openai`, `anthropic`)은 지금과 같게 한다. 0.2.x가 저장한 키를 읽을 수 있도록 맞추되, 읽지 못하면 사용자가 키를 다시 입력한다.
- 데이터 폴더는 `OMYPIC_DATA_DIR`가 있으면 그 값, 없으면 0.2.x와 같은 위치(`<사용자 앱 데이터>/omypic-frontend/omypic-data`. Windows는 `%APPDATA%`, macOS는 `~/Library/Application Support`)다. Tauri 기본 폴더(`com.omypic.app`)를 쓰지 않는 이유는 0.2.x가 이미 받아 둔 fastText 모델(약 1.2GB)을 다시 받지 않기 위해서다. 폴더 이름 `omypic-frontend`는 0.2.x(Electron)가 패키지 `package.json`의 `name`에서 만든 userData 폴더 이름이다(`productName`은 electron-builder 설정 안에만 있어 쓰이지 않았다). SQLite 파일도 이 폴더에 둔다. (작성자의 판단)
- 앱 식별자는 지금과 같은 `com.omypic.app`, 제품 이름은 `OMyPIc`다.
- macOS에는 마이크 사용 설명(`NSMicrophoneUsageDescription`)을 넣는다.
- `frontend/electron/`, `scripts/prepare-resources.mjs`, `electron`, `electron-builder`, `keytar` 관련 설정은 삭제한다.
- 오프라인 배포용 fastText 모델 번들(`OMYPIC_BUNDLE_EMBEDDING=1`)은 Tauri 리소스로 유지한다.

**로컬 모델 파일 캐시.** 지금은 Whisper와 Kokoro 모델을 Electron(Chromium)의 브라우저 캐시에 둔다. Tauri는 OS 기본 웹뷰를 쓰고, 웹뷰마다 캐시 동작이 다르다. 그래서 모델 파일 보관을 백엔드로 옮긴다. 백엔드에 모델 파일 경로(`/api/local-models/...`)를 두고, 처음 요청받은 파일은 Hugging Face에서 받아 데이터 폴더에 저장한 뒤 돌려주고, 그다음부터는 디스크에서 돌려준다. 화면 쪽 모델 라이브러리는 이미 모델 주소를 바꿀 수 있게 되어 있으므로 주소만 백엔드로 돌린다. 받을 수 있는 모델은 설정에 적힌 저장소로 제한한다. (작성자의 판단)

### 5.7 음성 변경

- TTS 모델 목록에서 `device-voice`를 뺀다. 무료 TTS는 `local-kokoro` 하나다. 저장된 설정이 `device-voice`이면 `local-kokoro`로 바꿔 읽는다.
- `local-kokoro`와 AI OFF 세션에서 문장을 읽는 규칙:
  - 영어 문장은 Kokoro가 읽는다.
  - 한국어 문장은 새 한국어 모델이 읽는다. 한글이 한 글자라도 있으면 한국어 문장으로 보는 지금 규칙을 유지한다. 그래서 영어 표현을 인용한 한국어 문장은 인용 부분까지 한국어 모델이 읽는다(지금 내장 음성이 읽는 방식과 같다). 문장 안에서 언어별로 잘라 이어 붙이지는 않는다. (작성자의 판단)
  - 모델을 쓸 수 없는 문장은 지금처럼 소리 없이 화면에만 보여 주고 세션을 계속한다.
- 한국어 모델은 한국어 문장이 처음 필요할 때(피드백 언어가 한국어인 세션을 시작할 때) 내려받는다. 영어만 쓰는 사용자는 받지 않는다.
- 문장 나누기, 언어 판별, 문장 단위 캐시, 미리 합성하기는 지금 구조를 유지한다. `deviceVoice.js`에서 `speechSynthesis`를 쓰는 부분만 없앤다.
- 서버 쪽 `VoiceModelCatalog`에서도 `device-voice`를 뺀다. `tts.text` 이벤트 방식은 그대로다.

**한국어 모델 후보.** 1순위는 Supertonic 2다. 앱이 이미 쓰는 `@huggingface/transformers` 4.3.2가 Supertonic 모델 형식을 지원하는 것을 소스에서 확인했다. 한국어를 지원하는 ONNX 판이 실제로 이 라이브러리에서 도는지, 내려받는 크기와 합성 속도는 이 환경에서 Hugging Face에 접속할 수 없어 아직 확인하지 못했다. 음성 작업을 시작할 때 먼저 짧게 시험해 보고, 한국어 예문을 읽은 음성 파일과 함께 결과를 사용자에게 알린 뒤 확정한다. 1순위가 안 되면 MMS-TTS 한국어 모델을 시험한다. 시험 기준은 한국어 발음, 한국어 문장에 섞인 영어 표현을 알아들을 수 있게 읽는지, 내려받는 크기, 문장 하나를 합성하는 시간이다.

한국어 모델이 확정되기 전에는 `device-voice`를 제거하지 않는다. 두 변경은 함께 들어간다. 시험을 할 수 없거나 두 후보가 모두 기준에 못 미치면 3단계를 멈추고 사용자에게 알린다. 그동안 한국어 문장은 지금처럼 내장 음성이 읽는다.

### 5.8 빌드와 릴리스

- `frontend/package.json`의 스크립트를 Tauri 기준으로 바꾼다(`npm run dev`는 Tauri 개발 실행, `npm run build:desktop`은 설치본 빌드). 버전은 `0.3.0`으로 올리고 `tauri.conf.json`과 맞춘다.
- `.github/workflows/release.yml`:
  - test: `cargo test`(workspace 전체)와 `npm test`.
  - build: Windows에서 NSIS, macOS에서 DMG. Java 설치 단계는 없앤다.
  - release: 지금과 같다.
- README를 새 구조에 맞게 고친다.

### 5.9 진행 순서

1. **백엔드 + SQLite.** crate별로 테스트와 함께 옮긴다(domain, application, infrastructure, presentation, server 순). 끝나면 브라우저에서 화면을 Rust 백엔드에 붙여 AI OFF 세션과 질문/룰북/오답노트/백업 화면을 직접 돌려 본다. 통과하면 Java 코드를 삭제한다.
2. **Tauri.** 앱 껍데기, 명령, 키 보관, 모델 파일 캐시, 릴리스 워크플로. Linux에서 빌드하고 실행해 창이 뜨고 백엔드가 응답하는지 확인한다. Electron 코드를 삭제한다.
3. **음성.** 한국어 모델 시험, `device-voice` 제거, 한국어 모델 연결.

단계가 끝날 때마다 커밋하고 브랜치에 올린다.

### 5.10 검증

| 대상 | 방법 | 이 환경에서 가능한가 |
| --- | --- | --- |
| 도메인, 유스케이스 | Java 테스트를 옮긴 Rust 단위 테스트 | 가능 |
| OpenAI, Anthropic 연동 | 가짜 HTTP 서버로 요청 본문과 응답 처리 확인 | 가능 (실제 API 호출은 불가) |
| HTTP API | axum 라우터에 직접 요청하는 테스트 | 가능 |
| SQLite, 백업 zip | 임시 DB 파일로 저장/조회, 0.2.x 형식 zip 가져오기 | 가능 |
| 화면 | 기존 vitest, 브라우저로 Rust 백엔드에 붙여 직접 조작 | 가능 |
| Tauri 앱 | Linux 빌드와 실행 | 시스템 패키지 설치가 되면 가능 |
| Windows, macOS 설치본 빌드 | 릴리스 워크플로를 버전 없이 수동 실행 | 사용자 동의 뒤 가능 |
| Windows, macOS에서의 실제 동작 | 사용자가 설치본을 실행 | 불가 |
| 로컬 음성 모델 | 실제 모델을 받아 합성 | Hugging Face 접속이 허용되면 가능 |

## 6. 만드는 사람에게 맡기는 것

- crate 안의 모듈 배치, 오류 타입, 비동기 trait를 쓰는 방식.
- SQLite 테이블의 열 구성과 인덱스.
- 1,649줄짜리 `VoiceSessionFacade`를 Rust에서 어떻게 나눌지. 밖에서 보이는 동작만 같으면 된다.
- Tauri의 CSP와 권한(capability) 설정의 구체적인 값.
- 테스트 케이스를 Rust 테스트 모듈에 배치하는 방식.

## 7. 확인이 필요한 위험

- Windows(WebView2)와 macOS(WKWebView)에서 마이크 권한 요청, Web Worker 안의 ONNX Runtime WebAssembly, 오디오 재생이 Electron과 같게 동작하는지는 실제 기기에서만 확인된다.
- macOS 웹뷰에서 `http://127.0.0.1` 호출이 막히면 화면을 백엔드가 직접 내려 주는 방식으로 바꿔야 한다. Linux 실행 검증으로는 이 문제가 드러나지 않는다.
- 0.2.x가 저장한 API 키를 새 앱이 읽지 못할 수 있다. 그 경우 사용자가 다시 입력한다.
- 한국어 모델의 음질과 속도가 기대에 못 미칠 수 있다. 시험 결과를 보고 사용자가 결정한다.
