# Tauri 데스크톱 앱 Implementation Plan (2/3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Electron을 Tauri v2로 바꾸고, Rust 백엔드를 Tauri 프로세스 안에서 실행한다.

**Architecture:** `frontend/src-tauri`의 Tauri 앱이 시작할 때 `omypic_server::start`를 빈 포트로 부르고, 화면은 지금처럼 `fetch`와 `EventSource`로 그 주소를 호출한다. `window.omypic` 인터페이스는 Tauri 명령을 부르는 연결 모듈이 만든다. 로컬 음성 모델 파일은 백엔드가 받아서 데이터 폴더에 보관한다.

**Tech Stack:** Tauri 2, `@tauri-apps/api` 2, `@tauri-apps/cli` 2, `keyring` 3, `rfd`(파일 대화상자), `dirs`. 계획 1의 crate.

**Spec:** `docs/superpowers/specs/2026-11-19-rust-migration-design.md` (5.6, 5.8)

**선행:** 계획 1(`2026-11-19-rust-backend-sqlite.md`)이 끝나 있어야 한다.

## Global Constraints

- 앱 식별자 `com.omypic.app`, 제품 이름 `OMyPIc`, 버전 `0.3.0`(`frontend/package.json`, `tauri.conf.json`, workspace crate 버전이 모두 같다).
- 설치본 대상: Windows NSIS(x64), macOS DMG(Apple Silicon). 서명하지 않는다.
- `window.omypic`의 메서드 이름과 반환 형태는 지금과 같다: `getBackendUrl() -> string`, `getApiKey(provider) -> string | null`, `setApiKey(key, provider) -> true`, `saveBackupFile(defaultName, bytes) -> boolean`, `pickBackupFile() -> {name, bytes: Uint8Array} | null`.
- API 키 보관: 서비스 `OMyPIc`, 계정 `openai` 또는 `anthropic`. 모르는 provider는 `openai`로 본다. 빈 문자열을 저장하면 항목을 지운다.
- 데이터 폴더: `OMYPIC_DATA_DIR`가 있으면 그 값, 없으면 `<dirs::config_dir()>/OMyPIc/omypic-data`.
- 설치본에 JRE, MongoDB, Node 런타임이 들어가지 않는다.
- 화면 코드는 연결 모듈과 모델 주소 설정 외에는 바꾸지 않는다. `npm test`가 통과해야 한다.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` 통과.

## Review Focus

1. **모델 내려받기가 중간에 끊긴 경우.** 반쯤 받은 파일이 캐시에 남아 다음 실행에서 깨진 모델로 읽히면 안 된다. → Task 1.
2. **허용하지 않은 주소 요청.** `/api/local-models/`로 설정에 없는 저장소나 `..`가 든 경로를 요청하면 거절해야 한다. → Task 1.
3. **0.2.x가 저장한 API 키.** Windows에서 keytar가 저장한 항목(대상 이름 `OMyPIc/openai`, UTF-8 바이트)을 읽어야 한다. 읽지 못하면 오류 대신 "키 없음"으로 처리한다. → Task 2.
4. **백엔드 시작 실패.** 데이터 폴더를 만들 수 없거나 DB를 열 수 없으면 빈 창 대신 오류 대화상자를 보여 주고 종료한다. → Task 2.
5. **파일 대화상자 취소.** 저장/열기 대화상자를 취소하면 각각 `false`, `null`을 돌려주고 화면이 멈추지 않는다. → Task 3.

---

### Task 1: 로컬 모델 파일 캐시

**Files:**
- Create: `backend/crates/infrastructure/src/local_models.rs`, `backend/crates/presentation/src/routes/local_models.rs`
- Modify: `backend/crates/infrastructure/src/config.rs`, `backend/crates/presentation/src/{lib.rs,state.rs}`, `backend/crates/server/src/wiring.rs`
- Modify: `frontend/src/features/voice/local/{localStt.js,localTts.js,localSttWorker.js,localTtsWorker.js}`와 각 테스트

**Interfaces:**
- Produces (Rust):
  - 설정: `local_models.upstream`(환경 변수 `OMYPIC_LOCAL_MODEL_UPSTREAM`, 기본 `https://huggingface.co`), `local_models.allowed_repos`(환경 변수 `OMYPIC_LOCAL_MODEL_REPOS`, 쉼표로 구분, 기본 `Xenova/whisper-base.en,onnx-community/Kokoro-82M-v1.0-ONNX`).
  - `LocalModelCache::new(config, data_dir)`, `fetch(&self, repo: &str, revision: &str, file: &str) -> AppResult<CachedFile { path: PathBuf, content_type: String }>`. 캐시 위치는 `<data_dir>/local-models/<repo>/<revision>/<file>`. 없으면 `{upstream}/{repo}/resolve/{revision}/{file}`에서 받아 `*.part`에 쓴 뒤 다 받으면 이름을 바꾼다. 같은 파일의 동시 요청은 한 번만 내려받는다.
  - 라우트 `GET /api/local-models/{org}/{name}/resolve/{revision}/{*file}`: 캐시된 파일을 스트리밍한다. `Content-Length`와 `Content-Type`을 넣는다. 허용 목록에 없는 저장소는 403, 경로 조각에 `..`가 있거나 비어 있으면 400, 상위 서버 404는 404, 그 밖의 상위 오류는 502.
- Produces (JS): `localStt.js`와 `localTts.js`의 기본 `modelHost`를 `${await getBackendUrl()}/api/local-models/`로 한다. `VITE_OMYPIC_LOCAL_STT_HOST` / `VITE_OMYPIC_LOCAL_TTS_HOST`가 있으면 지금처럼 그 값을 쓴다. 기본 주소를 쓸 때 워커에서 `env.useBrowserCache = false`로 한다(파일은 백엔드가 보관한다). `kokoro-js` 1.2.1이 음성 파일(`voices/*.bin`)을 어떤 주소에서 받는지 `node_modules/kokoro-js` 소스로 확인하고, Hugging Face 주소가 고정돼 있으면 TTS 워커 안에서 `fetch`를 감싸 `https://huggingface.co/` 접두사를 백엔드 주소로 바꾼다.

- [ ] **Step 1:** Rust 테스트(httpmock을 상위 서버로 사용):
  - `first_request_downloads_and_caches`: 첫 요청은 상위 서버를 1번 부르고, 둘째 요청은 부르지 않으며 두 응답의 바이트가 같다.
  - `partial_download_is_not_served`: 상위 서버가 `Content-Length`보다 적게 보내고 끊으면 오류이고, 캐시 폴더에 완성 파일이 없다. 다음 요청은 다시 내려받는다.
  - `concurrent_requests_download_once`: 같은 파일을 스레드 4개가 동시에 요청하면 상위 호출이 1번이다.
  - `rejects_repo_outside_allowlist`(403), `rejects_path_traversal`(`file`이 `../x`면 400), `upstream_404_maps_to_404`, `upstream_500_maps_to_502`.
  - `serves_wasm_and_onnx_content_types`: `.json` → `application/json`, `.onnx`/`.bin` → `application/octet-stream`.
- [ ] **Step 2:** 실패 확인, 구현, 통과 확인.
- [ ] **Step 3:** JS 테스트(`localStt.test.js`, `localTts.test.js`에 추가): 환경 변수가 없으면 워커에 넘기는 `config.modelHost`가 `http://localhost:4317/api/local-models/`이다(`getBackendUrl` mock). 환경 변수가 있으면 그 값이다.
- [ ] **Step 4:** 실패 확인, 구현, `npm test` 통과.
- [ ] **Step 5:** 커밋 `feat: 로컬 음성 모델 파일을 백엔드가 받아 보관하도록 변경`.

### Task 2: Tauri 앱

**Files:**
- Create: `frontend/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,Info.plist,capabilities/default.json}`, `frontend/src-tauri/src/{main.rs,lib.rs,paths.rs,keys.rs,backup.rs,backend.rs}`, `frontend/src-tauri/icons/*`
- Modify: `Cargo.toml`(workspace members에 `frontend/src-tauri` 추가)

**Interfaces:**
- Consumes: `omypic_server::start(config, bind) -> RunningServer`, `AppConfig`.
- Produces:
  - crate 이름 `omypic-desktop`(lib 이름 `omypic_desktop`). 실행 파일 이름은 `OMyPIc`.
  - `paths::resolve_data_dir(env: Option<String>, config_dir: Option<PathBuf>) -> PathBuf`.
  - `paths::resolve_bundled_embedding(resource_dir: &Path) -> Option<BundledModel { path, version, sha256 }>`: `models/` 아래에서 `manifest.json`을 뺀 첫 파일(이름순). `manifest.json`의 `filename`, `sha256`을 읽는다(`frontend/electron/main.cjs`의 `resolveBundledEmbeddingModel`과 같다).
  - `keys::resolve_provider(provider: Option<&str>) -> &'static str`, `keys::KeyStore` trait(`get`, `set`, `delete`)와 `keyring` 구현. Windows에서는 keytar와 같은 대상 이름(`OMyPIc/<account>`)을 쓰고, 저장된 값이 UTF-16으로 읽히지 않으면 UTF-8로 읽는다.
  - Tauri 명령: `get_backend_url() -> String`, `get_api_key(provider: Option<String>) -> Option<String>`, `set_api_key(key: Option<String>, provider: Option<String>) -> bool`, `save_backup_file(default_name: Option<String>, bytes: Vec<u8>) -> bool`, `pick_backup_file() -> Option<PickedFile { name: String, bytes: Vec<u8> }>`. 대화상자 필터는 zip, 기본 파일 이름 `omypic-backup.zip`.
  - 시작: `setup`에서 `OMYPIC_BACKEND_URL`이 있으면 내장 서버를 띄우지 않고 그 주소를 쓴다(개발용). 없으면 `AppConfig::from_env()`에 데이터 폴더와 번들 임베딩 모델 경로를 채워 `start(config, "127.0.0.1:0")`를 부른다. 실패하면 오류 대화상자(`OMyPIc startup error`, 메시지, "앱을 종료합니다.")를 보여 주고 종료 코드 1로 끝낸다. 창을 닫으면 `RunningServer::shutdown`을 부른다.
  - `tauri.conf.json`: `productName` `OMyPIc`, `identifier` `com.omypic.app`, `version` `0.3.0`, 창 1200x900, 제목 `OMyPIc`, `build.frontendDist` `../dist`, `build.devUrl` `http://localhost:5173`, `build.beforeDevCommand` `npm run dev:web`, `build.beforeBuildCommand` `npm run build`, `bundle.targets` `["nsis","dmg"]`, `bundle.resources` `["resources/**/*"]`, `bundle.category` `Education`, `bundle.macOS.infoPlist` 연결. CSP: `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob: data:; connect-src 'self' ipc: http://ipc.localhost http://127.0.0.1:* http://localhost:* https://huggingface.co`.
  - `Info.plist`: `NSMicrophoneUsageDescription` = `OPIc 답변을 녹음하기 위해 마이크를 사용합니다.`
  - 아이콘: 저장소에 앱 아이콘이 없다(Electron 기본 아이콘을 쓰고 있었다). 1024x1024 PNG(진한 남색 배경에 흰색 `O`)를 만들어 `npx tauri icon`으로 아이콘 묶음을 만든다.

- [ ] **Step 1:** 단위 테스트(`paths.rs`, `keys.rs`):
  - `data_dir_prefers_env`, `data_dir_defaults_to_config_dir_omypic`(`/c` → `/c/OMyPIc/omypic-data`), `data_dir_falls_back_to_current_dir_when_no_config_dir`.
  - `bundled_embedding_none_when_dir_missing`, `bundled_embedding_picks_first_file_and_reads_manifest`, `bundled_embedding_ignores_broken_manifest`.
  - `provider_defaults_to_openai`(`None`, `"x"` → `openai`; `"anthropic"` → `anthropic`).
  - 가짜 `KeyStore`로 `set_empty_key_deletes_entry`, `set_trims_key`, `get_missing_key_returns_none`.
  - `decodes_keytar_utf8_bytes_when_utf16_fails`: 바이트 `b"sk-abc"`를 넘기면 `"sk-abc"`다.
- [ ] **Step 2:** Linux 빌드에 필요한 시스템 패키지를 설치한다(`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libsoup-3.0-dev`, `libjavascriptcoregtk-4.1-dev`, `libsecret-1-dev` 등). 실패 확인, 구현, `cargo test -p omypic-desktop` 통과.
- [ ] **Step 3:** `cargo build -p omypic-desktop`이 통과한다.
- [ ] **Step 4:** 커밋 `feat: Tauri 앱 추가(내장 백엔드, 키 보관, 백업 파일 명령)`.

### Task 3: 화면 연결과 Electron 제거

**Files:**
- Create: `frontend/src/shared/desktop/tauriBridge.js`, `frontend/src/shared/desktop/tauriBridge.test.js`, `frontend/scripts/prepare-embedding.mjs`
- Modify: `frontend/src/main.jsx`, `frontend/package.json`, `frontend/package-lock.json`, `.gitignore`
- Delete: `frontend/electron/`, `frontend/scripts/prepare-resources.mjs`

**Interfaces:**
- Consumes: Task 2의 Tauri 명령.
- Produces:
  - `installTauriBridge(target = window, invoke) -> boolean`: `target.__TAURI_INTERNALS__`가 있으면 `target.omypic`을 Global Constraints의 인터페이스로 채우고 `true`, 없으면 아무것도 하지 않고 `false`. `main.jsx`가 렌더링 전에 부른다. `saveBackupFile`은 `bytes`를 `Array.from(new Uint8Array(bytes))`로 넘기고, `pickBackupFile`은 받은 `bytes`를 `Uint8Array`로 바꾼다.
  - `package.json` 스크립트: `dev:web`(`vite`), `dev`(`tauri dev`), `build`(`vite build`), `build:desktop`(`tauri build`), `build:desktop:win`(`tauri build --bundles nsis`), `build:desktop:mac`(`tauri build --bundles dmg`), `prepare:embedding`(`node scripts/prepare-embedding.mjs`), `test`, `test:watch`, `preview`. `main`과 `build`(electron-builder 설정) 항목을 지운다. 의존성에서 `keytar`, `electron`, `electron-builder`, `concurrently`, `wait-on`, `extract-zip`, `tar`를 빼고 `@tauri-apps/api`(dependencies), `@tauri-apps/cli`(devDependencies)를 넣는다. `version`은 `0.3.0`.
  - `prepare-embedding.mjs`: `OMYPIC_BUNDLE_EMBEDDING=1`일 때만 `OMYPIC_EMBEDDING_MODEL_SOURCE` 파일을 sha256 확인 후 `src-tauri/resources/models/`에 복사하고 `manifest.json`(`filename`, `sha256`)을 쓴다(`prepare-resources.mjs`의 임베딩 부분과 같은 동작). 아니면 `src-tauri/resources/`에 빈 `.gitkeep`만 둔다.

- [ ] **Step 1:** `tauriBridge.test.js`:
  - `does nothing outside Tauri`: `__TAURI_INTERNALS__`가 없으면 `false`이고 `target.omypic`이 없다.
  - `getApiKey forwards provider`: `invoke('get_api_key', {provider: 'anthropic'})`가 불린다.
  - `setApiKey forwards key and provider`.
  - `saveBackupFile sends plain number array and returns false on cancel`.
  - `pickBackupFile returns null on cancel`, `pickBackupFile converts bytes to Uint8Array`.
- [ ] **Step 2:** 실패 확인, 구현, `npm test` 통과.
- [ ] **Step 3:** Electron 파일과 의존성을 지우고 `npm install`로 lock 파일을 갱신한다. `npm test`, `npm run build` 통과.
- [ ] **Step 4:** Linux에서 실제 실행 확인. `npm run build:desktop -- --no-bundle`로 빌드하고 `xvfb-run`으로 실행한 뒤: 프로세스가 30초 동안 살아 있고, 로그에 백엔드 주소가 찍히고, 그 주소의 `/actuator/health`가 200이고, `<XDG_CONFIG_HOME>/OMyPIc/omypic-data/omypic.db`가 생긴다. 프로세스 목록에 `java`, `mongod`가 없다. 가능하면 WebDriver(`tauri-driver`)나 화면 캡처로 창에 화면이 그려진 것을 확인한다.
- [ ] **Step 5:** 커밋 `feat: 화면을 Tauri에 연결하고 Electron 제거`.

### Task 4: 릴리스 워크플로와 문서

**Files:**
- Modify: `.github/workflows/release.yml`, `README.md`

**Interfaces:**
- `release.yml`:
  - test: `actions/setup-node`, `dtolnay/rust-toolchain@stable`, Linux 시스템 패키지 설치(Task 2 Step 2의 목록), `cargo test --workspace`, `npm ci && npm test`.
  - build: `windows-latest`에서 `npm run build:desktop:win`, 산출물 `target/release/bundle/nsis/*.exe`; `macos-latest`에서 `npm run build:desktop:mac`, 산출물 `target/release/bundle/dmg/*.dmg`. `setup-java` 단계와 `CSC_IDENTITY_AUTO_DISCOVERY`를 지운다. `Swatinem/rust-cache@v2`를 쓴다.
  - release: 그대로.
- README: 구성(Backend: Rust/axum, Desktop: Tauri, 저장소: SQLite), 개발 실행(`cargo run -p omypic-server` + `npm run dev:web`, 또는 `npm run dev`), 패키징, 데이터 위치, **0.2.x에서 올 때는 업데이트 전에 설정 > 내보내기로 백업하고 0.3.0에서 가져오기**, API 키를 다시 입력해야 할 수 있다는 것, 로컬 모델이 `<데이터 폴더>/local-models`에 저장된다는 것, 환경 변수 표(`OMYPIC_MONGODB_URI`, `OMYPIC_MONGODB_BIN`, `OMYPIC_BACKEND_CMD` 삭제. `OMYPIC_PORT`, `OMYPIC_LOCAL_MODEL_UPSTREAM`, `OMYPIC_LOCAL_MODEL_REPOS` 추가).

- [ ] **Step 1:** `release.yml`을 고친다. YAML이 파싱되는지 확인한다(`python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`).
- [ ] **Step 2:** README를 고친다. 지금 README의 장 구성을 유지하고 내용만 바꾼다.
- [ ] **Step 3:** 커밋 `ci: 릴리스 워크플로와 문서를 Tauri 기준으로 변경`. push.
- [ ] **Step 4:** (사용자 동의가 있을 때만) 브랜치에서 Release 워크플로를 `version` 없이 수동 실행해 Windows와 macOS 설치본이 빌드되는지 확인한다. 실패하면 로그를 보고 고친다.
