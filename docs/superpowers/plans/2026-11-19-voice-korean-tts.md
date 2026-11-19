# 음성 변경 Implementation Plan (3/3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 운영체제 내장 음성(`device-voice`)을 없애고, 한국어 문장을 읽는 로컬 TTS 모델을 추가한다.

**Architecture:** 영어 문장은 지금처럼 Kokoro 워커가, 한국어 문장은 새 한국어 TTS 워커가 합성한다. 두 워커 모두 웹뷰의 Web Worker에서 `@huggingface/transformers`(ONNX Runtime WebAssembly)로 돌고, 모델 파일은 계획 2의 백엔드 캐시에서 받는다. 문장 나누기, 언어 판별, 문장 단위 캐시, 미리 합성하기는 지금 구조를 유지한다.

**Tech Stack:** `@huggingface/transformers` 4.3.2, `kokoro-js` 1.2.1, vitest.

**Spec:** `docs/superpowers/specs/2026-11-19-rust-migration-design.md` (5.7)

**선행:** 계획 1, 계획 2가 끝나 있어야 한다.

## Global Constraints

- TTS 모델 목록에서 `device-voice`를 뺀다. 무료 TTS의 id는 `local-kokoro` 하나다.
- 저장된 설정이 `device-voice`이면 `local-kokoro`로 읽는다.
- 영어 문장은 Kokoro, 한국어 문장(한글이 한 글자라도 있는 문장)은 한국어 모델이 읽는다. 문장 안에서 언어별로 자르지 않는다.
- 한국어 문장을 영어 음성이 읽거나, 영어 문장을 한국어 음성이 읽으면 안 된다.
- 모델을 쓸 수 없는 문장은 소리 없이 화면에만 보여 주고 세션을 계속한다.
- 한국어 모델은 한국어 문장이 처음 필요할 때 받는다. 피드백 언어가 영어뿐인 세션에서는 받지 않는다.
- **한국어 모델이 확정되기 전에는 `device-voice`를 제거하지 않는다.** Task 1이 사용자 확정으로 끝나지 않으면 Task 2 이후를 시작하지 않는다.

## Review Focus

1. **한국어 모델을 받지 못한 경우**(오프라인, 내려받기 실패). 한국어 문장은 조용히 화면 표시로 넘어가고 영어 문장은 계속 읽어야 한다. → Task 3.
2. **영어를 인용한 한국어 문장**(`"I goed" 대신 "I went"라고 해야 합니다.`). 문장 전체가 한국어 모델로 가고, 합성이 실패해도 세션이 멈추지 않는다. → Task 3.
3. **아주 긴 문장.** 한국어 모델의 입력 한도를 넘는 문장은 지금 `splitForSpeech`의 220자 분할로 나뉘어 들어간다. → Task 3.
4. **세션 도중 중지.** 한국어 합성이 진행 중일 때 세션을 멈추면 합성 결과가 재생되지 않는다. → Task 3.
5. **이전 설정값.** `device-voice`가 저장된 사용자가 0.3.0을 처음 켜면 설정 화면에 `local-kokoro`가 선택돼 있고 세션이 열린다. → Task 3.

---

### Task 1: 한국어 모델 시험과 확정 (사용자 확인 필요)

**Files:**
- Create: 작업용 폴더(scratchpad)의 시험 페이지와 스크립트. 저장소에는 결과 문서만 남긴다: `docs/013 Rust 전환/한국어 TTS 모델 시험.md`

- [ ] **Step 1:** `huggingface.co`와 모델 파일이 리다이렉트되는 호스트에 대한 네트워크 접근을 요청한다(사용자 승인). 거절되면 여기서 멈추고 사용자에게 알린다.
- [ ] **Step 2:** 1순위 후보(Supertonic 2의 ONNX 판, Hugging Face `onnx-community` 조직에서 한국어를 지원하는 Supertonic 저장소를 찾는다)를 Playwright Chromium의 Web Worker에서 `@huggingface/transformers` 4.3.2의 `pipeline('text-to-speech', …)`으로 돌린다. `device: 'wasm'`. 예문 5개를 합성해 wav로 저장한다:
  - `전반적으로 내용 전달은 좋았습니다.`
  - `과거 시제를 쓸 때 동사 형태에 주의하세요.`
  - `"I goed to the park" 대신 "I went to the park"라고 해야 합니다.`
  - `다음에는 구체적인 예시를 두 가지 이상 들어 보세요.`
  - `Overall the answer was clear.` (영어 문장. 비교용)
- [ ] **Step 3:** 기록한다: 저장소 id, 라이선스, 내려받는 총 크기(dtype별), 모델 준비 시간, 문장별 합성 시간, 출력 샘플레이트, 언어 지정 방법(태그나 옵션), 필요한 음성 파일.
- [ ] **Step 4:** 1순위가 돌지 않거나 한국어를 읽지 못하면 2순위(`Xenova/mms-tts-kor`)를 같은 방법으로 시험한다. 로마자 변환이 필요해 한국어 입력을 그대로 읽지 못하면 그 사실을 기록한다.
- [ ] **Step 5:** 결과 문서를 쓰고 wav 파일과 함께 사용자에게 보낸다. 추천 모델과 근거를 적는다. **사용자가 모델을 확정할 때까지 기다린다.** 두 후보가 모두 기준에 못 미치면 그렇게 보고하고 이 계획을 멈춘다.
- [ ] **Step 6:** 커밋 `docs: 한국어 TTS 모델 시험 결과 추가`.

Task 1의 산출물(아래 작업이 쓰는 값): `KO_TTS_MODEL`(저장소 id), `KO_TTS_DTYPE`, `KO_TTS_VOICE`(음성 파일 경로나 id), `KO_TTS_OPTIONS`(추론 단계 수 등), 언어 지정 방법.

### Task 2: 서버 쪽 모델 목록

**Files:**
- Modify: `backend/crates/application/src/voice/policy.rs`(`VoiceModelCatalog`)와 테스트, `backend/crates/infrastructure/src/config.rs`(`local_models.allowed_repos` 기본값에 `KO_TTS_MODEL` 추가), `backend/crates/presentation/src/routes/meta.rs`의 테스트

**Interfaces:**
- Produces: `VoiceModelCatalog::supported_tts_models()` = `["gpt-4o-mini-tts", "tts-1-hd", "tts-1", "local-kokoro"]`. `DEVICE_TTS_MODEL` 상수를 지운다. `is_client_tts("device-voice")`는 `true`를 유지한다(0.2.x 설정을 그대로 보낸 요청이 세션을 열 수 있도록. 서버는 어차피 `tts.text`만 보낸다). 세션 열기 검증에서 `device-voice`를 `local-kokoro`로 바꿔 받아들인다.

- [ ] **Step 1:** 테스트: `tts_models_do_not_list_device_voice`, `device_voice_request_is_treated_as_local_kokoro`(세션을 `ttsModel: "device-voice"`로 열면 성공하고 `tts.text` 이벤트가 나온다), `/api/meta/models` 응답의 `ttsModels`에 `device-voice`가 없다, `default_allowed_repos_include_korean_model`.
- [ ] **Step 2:** 실패 확인, 구현, `cargo test --workspace` 통과.
- [ ] **Step 3:** 커밋 `feat: TTS 모델 목록에서 기기 내장 음성 제거`.

### Task 3: 화면 쪽 음성

**Files:**
- Create: `frontend/src/features/voice/local/localKoTts.js`, `localKoTtsWorker.js`, `localKoTts.test.js`, `frontend/src/features/voice/local/speechText.js`(+ 테스트)
- Modify: `frontend/src/features/voice/local/localSpeech.js`(+ 테스트), `frontend/src/shared/constants/models.js`(+ 테스트), `frontend/src/shared/utils/preferences.js` 또는 TTS 설정을 읽는 곳(`frontend/src/app/hooks/useAppViewModel.js`), `frontend/src/features/settings/components/{ModelSettingsOverlay,SummaryPanel}.jsx`(+ 테스트), `frontend/src/features/voice/hooks/{useVoiceLifecycle,useTtsPlaybackQueue,bindVoiceEvents}.js`(+ `useVoiceSession.test.jsx`)
- Delete: `frontend/src/features/voice/local/deviceVoice.js`, `deviceVoice.test.js`

**Interfaces:**
- Consumes: Task 1의 `KO_TTS_*` 값, 계획 2의 모델 주소(`getBackendUrl()` + `/api/local-models/`).
- Produces:
  - `speechText.js`: `splitForSpeech(text) -> string[]`, `detectSpeechLang(text) -> 'ko-KR' | 'en-US'`(지금 `deviceVoice.js`의 두 함수를 그대로 옮긴다. 테스트도 옮긴다).
  - `localKoTts.js`: `isLocalKoTtsSupported() -> boolean`, `loadLocalKoTts(onProgress?) -> Promise<void>`, `synthesizeKorean(text) -> Promise<{samples: Float32Array, sampleRate: number}>`. 구조와 워커 메시지 형식은 `localTts.js` / `localTtsWorker.js`와 같다. 빌드 환경 변수 `VITE_OMYPIC_LOCAL_KO_TTS_MODEL`(기본 `KO_TTS_MODEL`), `VITE_OMYPIC_LOCAL_KO_TTS_VOICE`, `VITE_OMYPIC_LOCAL_KO_TTS_HOST`, `VITE_OMYPIC_LOCAL_KO_TTS_PATH_TEMPLATE`.
  - `localSpeech.js`: `SPEECH_ENGINE`을 없애고 `engine` 인자를 지운다. 문장 계획은 `{sentence, lang, synthesize}`이고 `synthesize`는 영어면 `synthesizeEnglish`, 한국어면 `synthesizeKorean`, 해당 모델을 쓸 수 없으면 `null`(화면 표시만)이다. `needsLocalTtsModel()`은 `{english: boolean, korean: boolean}`을 돌려주는 `neededLocalTtsModels(text 또는 feedbackLanguage)`로 바꾼다. 재생은 지금의 `playSamples` 경로 하나만 남는다.
  - `models.js`: `DEVICE_TTS_MODEL`과 그 표시 이름을 지운다. `local-kokoro`의 표시 이름은 `로컬 음성 (무료, 영어 Kokoro + 한국어)`. `normalizeTtsModel(value)`: `'device-voice'` → `'local-kokoro'`, 그 밖은 그대로.
  - 세션 시작 시 모델 준비: `local-kokoro` 또는 AI OFF면 Kokoro를 준비한다. 한국어 모델은 AI가 켜져 있고 TTS가 `local-kokoro`이며 피드백 언어가 `ko`일 때만 준비한다. 한국어 모델 준비가 실패해도 세션은 열린다.

- [ ] **Step 1:** 테스트를 쓴다.
  - `speechText.test.js`: `deviceVoice.test.js`의 `splitForSpeech`, `detectSpeechLang` 테스트를 옮긴다. 추가: `detects Korean when an English quote is embedded`(`"I goed" 대신 "I went"라고 해야 합니다.` → `ko-KR`), `splits a 500-character Korean sentence into chunks of at most 220`.
  - `localSpeech.test.js`(다시 쓴다): `English sentence goes to Kokoro`, `Korean sentence goes to the Korean model`, `mixed text routes each sentence by its own language`, `Korean sentence is skipped silently when the Korean model fails to load and English still plays`, `sentence is skipped when its synthesis rejects`, `stop() during Korean synthesis prevents playback`, `second read of the same sentence reuses the cached audio`, `speechSynthesis is never touched`(`window.speechSynthesis.speak` spy가 0번).
  - `localKoTts.test.js`: 설정 읽기(기본 모델 id, 환경 변수 우선, 기본 `modelHost`가 백엔드 주소), 워커 응답 처리(성공, 오류 메시지).
  - `models.test.js`: `TTS options do not include device-voice`, `normalizeTtsModel maps device-voice to local-kokoro`.
  - `ModelSettingsOverlay.test.jsx`, `SummaryPanel.test.jsx`: `기기 내장 음성` 항목이 없다. 저장값이 `device-voice`면 `local-kokoro`가 선택돼 있다.
  - `useVoiceSession.test.jsx`: 피드백 언어 `ko` + `local-kokoro` 세션은 시작할 때 두 모델을 준비한다. 피드백 언어 `en`이면 한국어 모델을 준비하지 않는다. AI OFF 세션은 Kokoro만 준비한다. 한국어 모델 준비가 실패해도 `session.ready` 뒤 질문이 진행된다.
- [ ] **Step 2:** `npm test` 실행, 실패 확인.
- [ ] **Step 3:** 구현한다. `deviceVoice.js`를 지우고 `speechSynthesis`, `SpeechSynthesisUtterance` 참조가 `frontend/src`에 남지 않게 한다(`grep -rn "speechSynthesis" frontend/src`가 테스트의 spy 외에 0건).
- [ ] **Step 4:** `npm test`, `npm run build` 통과.
- [ ] **Step 5:** 실제 실행 확인(Hugging Face 접근이 허용된 경우). `cargo run -p omypic-server` + `npm run dev:web`을 띄우고 Playwright Chromium에서 AI OFF 세션을 시작해 첫 질문이 Kokoro로 합성되는지(오디오 요소 재생, 콘솔 오류 없음) 확인한다. 한국어 합성은 화면의 개발용 진입점이 없으면 워커를 직접 불러 `전반적으로 내용 전달은 좋았습니다.`가 0.5초 이상의 오디오로 나오는지 확인한다. 모델 파일이 `<데이터 폴더>/local-models/` 아래에 생겼는지 확인한다.
- [ ] **Step 6:** 커밋 `feat: 기기 내장 음성 제거, 한국어 로컬 TTS 추가`.

### Task 4: 문서

**Files:**
- Modify: `README.md`(STT/TTS 장)

- [ ] **Step 1:** STT/TTS 표에서 `device-voice` 행을 지우고 `local-kokoro` 설명을 고친다(영어는 Kokoro, 한국어는 확정된 모델, 각 모델의 내려받는 크기, 한국어 모델은 한국어 피드백 세션에서만 받는다는 것, 새 환경 변수 `VITE_OMYPIC_LOCAL_KO_TTS_*`). AI OFF 설명에서 "기기 내장 음성"을 "로컬 음성"으로 고친다. Windows에 영어 음성을 설치하라는 안내를 지운다.
- [ ] **Step 2:** 커밋 `docs: README의 음성 설명을 로컬 모델 기준으로 변경`. push.
