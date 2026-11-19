# OMyPIc 데스크톱 앱 실행/패키징 안내

이 문서는 OMyPIc를 Tauri 기반 단일 실행 앱으로 개발/배포하는 방법을 설명합니다.

## 1) 구성

- Backend: Rust(axum) 서버 `omypic-server` (`backend/crates/`). 설치본에서는 앱 프로세스 안에서 돌아갑니다.
- 데이터 저장: SQLite 파일 하나(`<데이터 폴더>/omypic.db`)
- Frontend: React + Vite (`frontend/`)
- Desktop: Tauri v2 (`frontend/src-tauri/`, crate `omypic-desktop`). 워크스페이스(`Cargo.toml`)의 멤버입니다.
- 설치본에는 별도 런타임이나 DB 서버가 들어가지 않습니다. 선택 사항인 임베딩 모델만 `frontend/src-tauri/resources/models/`에 담깁니다.

## 2) 개발 실행

### 준비물

- Node.js: `>= 20.19.0` (프로젝트 루트 `.nvmrc` 참고)
- Rust: 프로젝트 루트 `rust-toolchain.toml`에 고정된 버전(rustup이 자동으로 맞춥니다)
- Linux에서 빌드할 때는 아래 패키지가 필요합니다. API Key 저장에는 실행 중인 Secret Service(예: gnome-keyring)도 필요합니다.

  ```bash
  sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev libsecret-1-dev libssl-dev pkg-config build-essential xdg-utils
  ```

- Windows: MSVC 빌드 도구와 WebView2가 필요합니다. WebView2는 Windows 10/11에 들어 있고, 없는 PC에서는 NSIS 설치본이 기본 부트스트래퍼로 설치합니다(인터넷 연결 필요).
- macOS: Xcode 명령줄 도구가 필요합니다.

### 실행

```bash
cd frontend
npm ci

# 데스크톱 앱(개발 모드): Vite dev 서버와 내장 Backend를 함께 띄웁니다.
npm run dev
```

- `npm run dev`는 `tauri dev`입니다. `beforeDevCommand`로 Vite(`http://localhost:5173`)를 띄우고, 디버그 앱이 빈 포트에 내장 Backend를 올립니다.
- Backend를 따로 띄워 쓰려면 `OMYPIC_BACKEND_URL=http://127.0.0.1:4317`을 주고 앱을 실행합니다. 이 변수가 있으면 내장 Backend를 띄우지 않습니다.
- 브라우저만으로 화면을 볼 때(키 저장소와 백업 파일 대화상자는 쓸 수 없습니다):

  ```bash
  # terminal 1 (저장소 루트에서)
  cargo run -p omypic-server

  # terminal 2
  cd frontend
  npm run dev:web
  ```

- 단독 Backend 기본 주소: `http://127.0.0.1:4317` (`127.0.0.1`에만 바인딩, 포트는 `OMYPIC_PORT`로 변경). `npm run dev:web`은 Backend가 이 주소에 있다고 보고 연결합니다.
- Backend 테스트: `cargo test --workspace`
- 화면 테스트: `cd frontend && npm test`
- 검사: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`

### 요청 검사(개발 중 막힐 수 있는 부분)

Backend는 브라우저의 아무 페이지나 `127.0.0.1`로 요청을 보낼 수 있다는 점을 막기 위해 요청마다 아래를 검사합니다.

- `Host`가 `127.0.0.1`, `localhost`, `[::1]`(포트는 아무거나)가 아니면 403입니다.
- `Origin`이 붙어 있는데 허용 목록에 없으면 403입니다. `Origin`이 없는 요청(curl 등)은 통과합니다.
- 허용 목록 기본값은 `http://localhost:5173`, `http://127.0.0.1:5173`, `tauri://localhost`, `http://tauri.localhost`, `https://tauri.localhost`입니다. 다른 주소(예: 다른 포트의 Vite)에서 화면을 열면 `OMYPIC_ALLOWED_ORIGINS`에 그 origin을 넣어야 합니다. 이 변수를 지정하면 기본 목록을 대신하므로 기본값도 함께 적어야 합니다.

### 앱 동작

- 앱은 한 번에 하나만 실행됩니다. 이미 실행 중일 때 다시 실행하면 새 프로세스는 끝나고 기존 창이 앞으로 나옵니다.
- 창을 닫으면 Backend를 멈추고 앱이 종료됩니다. macOS에서도 창을 닫으면 앱이 종료됩니다(창만 닫히고 앱이 남지 않습니다).
- 시작에 실패하면(예: 잘못된 환경 변수) `OMyPIc startup error` 대화상자에 원인을 보여 주고 종료합니다.

## 3) 배포 패키징

```bash
cd frontend
npm ci

# 현재 플랫폼에서 tauri.conf.json의 대상(NSIS, DMG)을 빌드
npm run build:desktop

# Windows 설치형 (NSIS, x64)
npm run build:desktop:win

# macOS 설치형 (DMG, Apple Silicon)
npm run build:desktop:mac

# 설치본 없이 실행 파일만 빌드
npm run build:desktop -- --no-bundle

# 오프라인 배포용: 임베딩 모델 번들 포함
OMYPIC_BUNDLE_EMBEDDING=1 \
OMYPIC_EMBEDDING_MODEL_SOURCE=/absolute/path/to/cc.ko.300.vec.gz \
OMYPIC_EMBEDDING_MODEL_SHA256=<sha256> \
OMYPIC_EMBEDDING_MODEL_VERSION=cc.ko.300.vec.gz \
npm run build:desktop
```

- `tauri build`는 먼저 `npm run build`(Vite)와 `npm run prepare:embedding`을 실행합니다(`tauri.conf.json`의 `beforeBuildCommand`). `prepare:embedding`만 따로 실행할 수도 있습니다.
- 산출물은 워크스페이스 루트의 `target/release/bundle/` 아래에 생깁니다(`frontend/` 아래가 아닙니다).
  - Windows: `target/release/bundle/nsis/*.exe`
  - macOS: `target/release/bundle/dmg/*.dmg`
  - 실행 파일만 빌드하면 `target/release/OMyPIc`입니다.
- NSIS와 DMG는 각각 Windows와 macOS에서만 빌드됩니다. Linux에서는 `--no-bundle`로 실행 파일까지만 만들 수 있습니다.
- Windows 설치본은 WebView2가 없는 PC에서 NSIS 기본 부트스트래퍼로 설치합니다.
- Intel Mac용 DMG는 워크플로에 포함되어 있지 않습니다. 필요하면 Intel Mac에서 `npm run build:desktop:mac`으로 빌드합니다.

### 임베딩 모델 번들 변수

`frontend/scripts/prepare-embedding.mjs`가 읽는 빌드 시 환경 변수입니다.

- `OMYPIC_BUNDLE_EMBEDDING`
  - `1`이면 임베딩 모델 파일을 `frontend/src-tauri/resources/models`에 포함. 아니면 아무것도 포함하지 않습니다.
- `OMYPIC_EMBEDDING_MODEL_SOURCE`
  - 번들 포함할 모델 파일 경로(`OMYPIC_BUNDLE_EMBEDDING=1`이면 필수). 상대 경로는 저장소 루트 기준입니다.
- `OMYPIC_EMBEDDING_MODEL_SHA256`
  - 번들 파일 sha256 검증값(권장). 맞지 않으면 빌드가 실패합니다.
- `OMYPIC_EMBEDDING_MODEL_VERSION`
  - 번들 파일명 override(기본: source 파일명)

번들한 모델은 `manifest.json`(`filename`, `sha256`)과 함께 설치본에 들어가고, 앱이 시작할 때 읽어 모델 경로, 버전, 해시를 채웁니다. 같은 값을 환경 변수(`OMYPIC_RAG_MODEL_PATH` 등)로 직접 주면 그 값이 우선합니다.

### GitHub Actions 릴리스

`.github/workflows/release.yml`이 설치본을 대상 OS의 러너에서 빌드합니다.

1. `frontend/package.json`, `frontend/src-tauri/tauri.conf.json`, 워크스페이스 `Cargo.toml`의 `version`을 같은 값으로 올립니다.
2. 아래 둘 중 하나로 워크플로를 실행합니다.
   - 태그 푸시: `git tag v0.3.0 && git push origin v0.3.0`
   - 수동 실행: Actions > Release > Run workflow에서 `version`에 `0.3.0`을 입력합니다. 태그는 초안을 게시할 때 그 커밋에 만들어집니다.
3. 워크플로가 테스트(`cargo fmt --check`, `cargo clippy`, `cargo test --workspace`, `npm test`) 후 Windows(NSIS, x64)와 macOS(DMG, Apple Silicon) 설치본을 빌드하고, 릴리스 초안에 첨부합니다.
4. GitHub Releases에서 초안을 확인한 뒤 직접 게시합니다. 버전에 `-`가 있으면(예: `0.3.0-rc.1`) 사전 릴리스로 표시됩니다.

`version`을 비우고 수동 실행하면 릴리스 없이 설치본만 빌드해 실행 결과(Artifacts)에 남깁니다.

## 4) 런타임 기동 순서

패키지된 앱 실행 시 Tauri 메인 프로세스는 아래 순서로 동작합니다.

1. 로그 설정(`<데이터 폴더>/logs/omypic.log`, stderr). 이미 실행 중인 앱이 있으면 새 프로세스는 그 창을 앞으로 가져오고 끝납니다(단일 인스턴스).
2. `OMYPIC_BACKEND_URL`이 있으면 그 서버를 사용하고, 없으면 환경 변수로 설정을 읽어 내장 Backend를 `127.0.0.1`의 빈 포트에 띄웁니다.
3. Backend가 요청을 받을 수 있게 되면 창을 보여 주고 프런트엔드가 `getBackendUrl()`로 주소를 받습니다.
4. 창을 닫으면 창을 숨기고 Backend를 종료한 뒤 프로세스가 끝납니다.


### 데이터 위치

설치본의 데이터 폴더는 0.2.x와 같은 곳입니다(`OMYPIC_DATA_DIR`로 바꿀 수 있습니다). 폴더 이름은 이전 버전이 쓰던 `omypic-frontend`를 그대로 씁니다. 그래서 0.2.x가 받아 둔 임베딩 모델(`models/`)을 다시 받지 않습니다.

- Windows: `%APPDATA%\omypic-frontend\omypic-data`
- macOS: `~/Library/Application Support/omypic-frontend/omypic-data`
- Linux: `~/.config/omypic-frontend/omypic-data`

폴더 안에는 아래가 생깁니다.

- `omypic.db`: SQLite 데이터베이스
- `local-models/`: 로컬 Whisper/Kokoro 모델 파일(Backend가 내려받아 보관)
- `logs/omypic.log`: 앱 로그. 시작할 때 5 MB를 넘었으면 `omypic.log.1`로 옮기고 새로 씁니다. API Key는 기록하지 않습니다.
- 룰북 파일, 임베딩 모델

### 0.2.x에서 올 때 (Upgrading from 0.2.x)

0.3.0은 0.2.x의 MongoDB 데이터 파일을 읽지 못합니다. 데이터는 백업 파일로 옮깁니다.

1. 0.2.x를 업데이트하기 전에 설정 > 내보내기로 백업 파일을 저장합니다.
2. 0.3.0을 설치하고 설정 > 가져오기로 그 백업을 불러옵니다.
3. 이미 업데이트했다면 0.2.x를 다시 설치해 내보내기를 하세요. 예전 MongoDB 폴더(같은 위치의 `omypic-frontend/db`)는 건드리지 않으므로 그대로 남아 있습니다.
4. API Key는 다시 입력해야 할 수 있습니다. 0.3.0은 0.2.x와 같은 OS 보안 저장소 항목을 읽도록 만들었지만 Windows와 macOS에서 확인하지는 못했습니다.
5. 룰북 파일이 UTF-8이 아닌 백업은 가져오기가 거부되고, 메시지에 해당 파일 이름이 나옵니다. 그 룰북을 UTF-8로 저장해 다시 내보내거나 가져오세요.

## 5) 환경 변수

### 앱/Backend 공통

- `OMYPIC_BACKEND_URL`
  - 기본값: 미지정. 지정하면 데스크톱 앱이 내장 Backend를 띄우지 않고 그 주소를 씁니다(개발용).
- `OMYPIC_PORT`
  - 기본값: `4317` (단독 `omypic-server`가 `127.0.0.1:<port>`에 리슨)
  - 데스크톱 앱의 내장 Backend는 이 값을 쓰지 않고 빈 포트를 골라 프런트엔드에 알려 줍니다.
- `OMYPIC_DATA_DIR`
  - 기본값: 단독 실행은 `<홈>/OMyPIc`, 데스크톱 앱은 `<OS 설정 폴더>/omypic-frontend/omypic-data`
  - SQLite 파일(`omypic.db`), 룰북 파일, 임베딩 모델, `local-models/`, `logs/`가 이 폴더에 저장됩니다.
- `OMYPIC_RAG_MODEL_PATH`
  - 기본값: 미지정
  - 설치본에 번들 모델이 있으면 앱이 경로를 자동으로 채웁니다.
- `OMYPIC_RAG_PROVIDER`
  - 기본값: `fasttext` (`hash`면 모델 없이 해시 임베딩)
- `OMYPIC_RAG_MODEL_VERSION`
  - 기본값: `cc.ko.300.vec.gz`
  - 번들 모델이 있으면 `manifest.json`의 파일명으로 채워집니다.
- `OMYPIC_RAG_MODEL_SHA256`
  - 기본값: `cc.ko.300.vec.gz` 고정 SHA-256
  - 번들 모델이 있으면 `manifest.json`의 값으로 채워집니다.
- `OMYPIC_RAG_DOWNLOAD_URL`
  - 기본값: fastText 공식 배포 URL (`cc.ko.300.vec.gz`)
- `OMYPIC_RAG_ALLOW_HASH_FALLBACK`
  - 기본값: `false`
- `OMYPIC_OPENAI_BASE_URL`
  - 기본값: `https://api.openai.com`
- `OMYPIC_OPENAI_TIMEOUT_SECONDS`
  - 기본값: `300` (최소 `1`)
  - OpenAI LLM/STT/TTS 요청 하나의 전체 제한 시간(초). 넘기면 연결 실패와 같은 오류로 처리합니다.
- `OMYPIC_ALLOWED_ORIGINS`
  - 기본값: `http://localhost:5173,http://127.0.0.1:5173,tauri://localhost,http://tauri.localhost,https://tauri.localhost`
  - Backend가 요청을 받아 주는 브라우저 origin(쉼표로 구분). 지정하면 기본 목록을 대신하며, 목록에 없는 `Origin`이 붙은 요청은 403입니다. `*`나 경로가 붙은 값은 설정 오류로 시작이 실패합니다.
- `OMYPIC_LOCAL_MODEL_UPSTREAM`
  - 기본값: `https://huggingface.co`
  - 로컬 Whisper/Kokoro 모델 파일을 내려받는 상위 서버(`{주소}/{org}/{name}/resolve/{revision}/{file}`).
- `OMYPIC_LOCAL_MODEL_REPOS`
  - 기본값: `Xenova/whisper-base.en,onnx-community/Kokoro-82M-v1.0-ONNX`
  - Backend가 내려받아 줄 모델 저장소(`org/name`, 쉼표로 구분, 대소문자 구분). 목록에 없는 저장소는 403입니다.
- `RUST_LOG`
  - 로그 수준. 기본값: `info`. 데스크톱 앱은 stderr와 `logs/omypic.log`에 함께 기록합니다.

별도 설정이 없으면 룰북 검색에 처음 쓸 때 모델이 자동 다운로드됩니다.
모델 파일이 크기 때문에(약 1.27GB) 네트워크 환경에 따라 첫 사용 때 시간이 걸릴 수 있습니다.

## 6) 코드서명 정책

- 현재 빌드는 개발/사내 배포용 unsigned 아티팩트를 기준으로 합니다.
- Windows 코드서명, macOS notarization은 후속 단계에서 적용합니다.

## 7) API Key 저장

- API Key는 앱이 OS 보안 저장소(Windows 자격 증명 관리자, macOS 키체인, Linux Secret Service)에 서비스 `OMyPIc`, 계정 `openai`/`anthropic` 항목으로 저장합니다. 빈 값을 저장하면 항목을 지웁니다.
- Backend에는 저장하지 않고 요청 시 헤더로 전달됩니다.
  - `X-API-Key`: OpenAI API Key. OpenAI의 STT, TTS, GPT 피드백 모델 중 하나라도 쓸 때만 필요합니다.
  - `X-Feedback-API-Key`: Claude API Key. Claude 피드백 모델을 선택하고 AI가 켜져 있을 때만 전송합니다.
- 키가 필요 없는 조합:
  - AI를 끈 세션: 키가 전혀 필요 없습니다.
  - Claude 피드백 + 로컬 Whisper(STT) + 기기 내장 음성 또는 로컬 Kokoro(TTS): Claude 키만 필요합니다.
- 로컬 Whisper와 Kokoro 모델 파일은 처음 쓸 때 Backend가 내려받아 `<데이터 폴더>/local-models`에 보관합니다.
- 설정 화면의 `저장 + 검증`은 두 키를 각각 해당 공급자에 확인합니다. Claude 키는 모델 조회(`GET /v1/models/{id}`)로 확인하므로 토큰을 쓰지 않습니다.

## 8) 피드백 모델 지원 범위

- OpenAI 모델:
  - `gpt-5-mini`
  - `gpt-5-nano`
  - `gpt-4.1`
  - `gpt-4.1-mini`
  - `gpt-4.1-nano`
  - `gpt-4o`
  - `gpt-4o-mini`
- Claude 모델:
  - `claude-opus-5-5`
  - `claude-sonnet-5-5`
  - `claude-haiku-5-5`
  - `claude-fable-5-2`
- 기본 피드백 모델: `gpt-5-nano` (Claude를 고르면 공급자 기본값은 `claude-opus-5-5`)
- 기본 설정(`gpt-5-nano`) 기준 세션 1회당 비용은 약 50원 수준이며, 실제 비용은 발화 길이/선택 모델에 따라 달라질 수 있습니다.
- 비용 정책상 `gpt-5-mini`보다 상위 GPT-5 계열(`gpt-5`, `gpt-5.1`, `gpt-5.2`, `gpt-5-pro`)은 지원 대상에서 제외됩니다.
- Claude 호출 방식:
  - Backend가 Messages API(`POST /v1/messages`)를 직접 호출하고, 응답 형식은 structured outputs(JSON Schema)로 고정합니다.
  - Claude는 서버에 대화를 보관하지 않으므로 매 턴 시스템 프롬프트를 함께 보내고, 세션 기억(요약/최근 턴/추천 표현)은 사용자 프롬프트에 붙입니다.
  - `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-fable-5-2`는 거절 응답에 대비해 서버 측 fallback(`fallbacks: "default"`)을 켭니다.
  - 음성 턴이 이 호출을 기다리므로 effort 기본값은 `low`입니다. 아래 환경 변수로 바꿀 수 있습니다.
- Claude 관련 환경 변수:
  - `OMYPIC_ANTHROPIC_BASE_URL` (기본값: `https://api.anthropic.com`)
  - `OMYPIC_ANTHROPIC_EFFORT` (기본값: `low`, 허용값: `low`/`medium`/`high`/`xhigh`/`max`)
  - `OMYPIC_ANTHROPIC_MAX_TOKENS` (기본값: `16000`)
  - `OMYPIC_ANTHROPIC_TIMEOUT_SECONDS` (기본값: `120`)
  - `OMYPIC_ANTHROPIC_MAX_RETRIES` (기본값: `2`)

### 음성 인식(STT)과 음성 합성(TTS)

Claude API에는 음성 인식/합성 기능이 없습니다. 그래서 STT/TTS는 피드백 모델과 따로 고르며, 무료 로컬 옵션을 함께 제공합니다.

| 설정 값 | 종류 | 실행 위치 | 비용 |
| --- | --- | --- | --- |
| `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`, `whisper-1` | STT | OpenAI API | 유료 |
| `local-whisper` | STT | 기기(앱 안에서 Whisper 실행) | 무료 |
| `gpt-4o-mini-tts`, `tts-1-hd`, `tts-1` | TTS | OpenAI API | 유료 |
| `device-voice` | TTS | 기기(운영체제 내장 음성) | 무료 |
| `local-kokoro` | TTS | 기기(앱 안에서 Kokoro 실행) | 무료 |

- `local-whisper`
  - 렌더러의 Web Worker에서 `@huggingface/transformers`(ONNX Runtime WebAssembly)로 Whisper를 실행합니다.
  - 답변 오디오는 기기를 떠나지 않습니다. 인식된 텍스트만 `POST /api/voice/sessions/{id}/answers`로 보냅니다.
  - 인식 모델 파일은 처음 사용할 때 Backend가 내려받아 `<데이터 폴더>/local-models`에 보관하고, 이후에는 그 파일을 읽습니다. 세션 시작 시 모델을 먼저 준비합니다.
  - ONNX Runtime 바이너리(`.wasm`)는 앱 번들에 포함되므로 CDN을 호출하지 않습니다.
  - 빌드 시 환경 변수로 모델을 바꿀 수 있습니다.
    - `VITE_OMYPIC_LOCAL_STT_MODEL` (기본값: `Xenova/whisper-base.en`)
    - `VITE_OMYPIC_LOCAL_STT_HOST` (기본값: Hugging Face. 사내 미러나 앱에 동봉한 모델 경로를 지정할 때 사용)
    - `VITE_OMYPIC_LOCAL_STT_PATH_TEMPLATE` (기본값: `{model}/`, `VITE_OMYPIC_LOCAL_STT_HOST`를 지정했을 때만 적용)
- 기기에서 읽어 주기(`device-voice`, `local-kokoro`, AI OFF 세션)
  - 서버는 오디오 대신 읽을 문장을 `tts.text` 이벤트로 보내고, 앱이 문장 단위로 읽습니다.
  - 문장마다 언어를 판별해 그 언어의 음성으로만 읽습니다. 영어 문장을 한국어 음성으로 읽지 않습니다.
  - 읽을 수단이 전혀 없으면 세션을 끊지 않고 질문을 화면으로만 보여 준 뒤 답변을 받습니다.
- `device-voice`
  - 운영체제에 설치된 음성(Web Speech `speechSynthesis`)으로 바로 읽습니다.
  - 기기에 영어 음성이 없으면(한국어 Windows 기본 상태 등) 영어 문장만 `local-kokoro`로 읽습니다. 이때 세션 시작 시 모델을 준비합니다.
  - 운영체제에 영어 음성을 설치하면 기다림 없이 읽습니다(Windows: 설정 > 시간 및 언어 > 음성 > 음성 추가에서 영어(미국)).
- `local-kokoro`
  - 영어 문장을 Kokoro 모델(`kokoro-js`, ONNX Runtime WebAssembly)로 합성해 원어민 발음으로 읽습니다. 한국어 문장은 기기 내장 음성으로 읽습니다.
  - 처음 사용할 때 모델(약 90MB)과 음성 파일을 Backend가 Hugging Face에서 내려받아 `<데이터 폴더>/local-models`에 보관합니다.
  - CPU에서 합성하므로 문장을 준비하는 데 시간이 걸립니다. 문장이 도착하는 즉시 합성을 시작하고, 같은 문장은 다시 합성하지 않습니다(질문 두 번째 재생은 즉시).
  - 빌드 시 환경 변수: `VITE_OMYPIC_LOCAL_TTS_MODEL`(기본값 `onnx-community/Kokoro-82M-v1.0-ONNX`), `VITE_OMYPIC_LOCAL_TTS_VOICE`(기본값 `af_heart`), `VITE_OMYPIC_LOCAL_TTS_HOST`, `VITE_OMYPIC_LOCAL_TTS_PATH_TEMPLATE`.

### AI ON/OFF

- 실전 연습 화면과 설정 화면의 `AI` 스위치로 전환하며, 다음 세션 시작부터 적용됩니다.
- OFF로 시작한 세션은 어떤 API도 호출하지 않습니다.
  - 음성 인식(STT)과 피드백 생성(부트스트랩/룰북 프리페치/오답노트 기록 포함)을 하지 않습니다.
  - 질문은 기기에서 읽어 줍니다. TTS 모델이 `local-kokoro`면 Kokoro로, 그 밖에는 기기 내장 음성으로 읽습니다(영어 음성이 없으면 Kokoro).
  - API Key 없이 시작할 수 있습니다(`X-API-Key` 헤더 불필요).
  - 답변 녹음은 기기에만 남습니다. 서버에는 오디오를 올리지 않고 답변 종료 신호만 보냅니다.
  - 흐름: `질문 읽어 주기 -> 답변 녹음 -> (내 응답 자동 재생) -> 다음 질문`.
- `답변 후 내 응답 자동 재생`을 켜 두면 답변이 끝난 직후 방금 녹음한 응답을 재생하고, 재생이 끝난 뒤 다음 질문을 읽습니다.
- API:
  - `POST /api/voice/sessions` 본문의 `aiEnabled`(기본 `true`). `false`면 키와 모델 검증을 생략합니다.
  - `POST /api/voice/sessions/{voiceSessionId}/answers` 본문 `{"sequence": n, "text": "..."}`: 오디오 업로드 없이 답변 종료를 알립니다.
    - AI OFF 세션: `text`는 쓰지 않습니다. 서버는 `answer.captured` 이벤트를 보낸 뒤 다음 질문을 보냅니다.
    - `local-whisper` 세션: `text`가 인식 결과입니다. 서버는 `stt.final` 이후 평소와 같은 피드백 흐름을 진행합니다.
    - 서버 STT 세션에서는 거부합니다(400). 같은 `sequence`는 한 번만 처리합니다.

## 9) 음성 청크 전송 보장

- 클라이언트는 음성 turn 청크를 HTTP `2xx` 응답 전까지 유지합니다(at-least-once).
- 업로드 실패 시 지수 백오프로 재시도합니다: `0.5s -> 1s -> 2s -> 4s -> 4s` (최대 5회).
- 같은 청크는 동일 `sequence`로 재전송되며, 서버는 세션 단위로 중복 `sequence`를 무시합니다.
- turn 캡처 길이는 최대 2분 30초로 제한되며, 초과 시 세션을 종료해 메모리 사용 폭증을 방지합니다.
