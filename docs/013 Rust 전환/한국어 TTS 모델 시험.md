# 한국어 TTS 모델 시험

시험일: 2026-11-20. 목적: 앱이 이미 쓰는 `@huggingface/transformers` 4.3.2와 ONNX Runtime WebAssembly(`device: 'wasm'`)로 한국어 음성 합성 모델을 기기 안 Web Worker에서 돌릴 수 있는지 확인하고, 모델을 고를 근거(음성 파일과 측정값)를 만든다.

## 결론

- **추천: `onnx-community/Supertonic-TTS-2-ONNX`, fp32, 추론 5단계, 속도 1.05, 음성 F2(잠정).** 브라우저 Worker에서 그대로 돌았고, 한국어 예문 세 개는 Whisper 받아쓰기가 입력과 같게 나왔다. 내려받기 263 MB, 모델 준비 약 2.5초, 한 문장 합성 약 1.1~1.8초(이 컨테이너, 단일 스레드).
- 대안: `open-world-group/Supertonic-3-ONNX`(Supertonic 3를 제3자가 transformers.js 형식으로 다시 묶은 것). 한국어는 같은 수준으로 받아써졌고 q8 구성은 내려받기가 207 MB로 더 작지만, 합성은 약 1.6배 느리고 출처를 확인할 수 없다(아래 참고). 귀로 들어 보고 Supertonic 3 쪽이 확실히 낫다고 판단될 때만 고려한다.
- 2순위 후보 `Xenova/mms-tts-kor`는 1순위가 동작했으므로 시험하지 않았다.
- 한 입력 안에서 `<ko>…</ko><en>…</en>`처럼 구간마다 언어를 바꾸는 방식은 두 모델 모두 동작하지 않는다(거의 무음이나 엉뚱한 소리). 영어 인용이 섞인 한국어 문장은 문장 전체를 `<ko>`로 감싸서 읽히는 것이 가장 나았다.

**직접 들어 보지 못했다.** 이 작업 환경에서는 소리를 들을 수 없어서, 길이·음량·클리핑 검사와 Whisper 받아쓰기로만 확인했다. 받아쓰기가 맞는다는 것은 알아들을 수 있다는 간접 증거일 뿐이고, 자연스러움·억양·음색은 판단하지 못했다. 음성 선택과 최종 확정은 `out/`의 wav를 직접 들어 보고 해야 한다.

## 시험 방법

- 환경: 컨테이너 CPU 4개(Intel Xeon 2.10GHz), GPU 없음. Playwright Chromium 157(헤드리스). 시간은 사용자 노트북과 다르므로 상대 비교로만 본다.
- 앱과 같은 구성:
  - `@huggingface/transformers` 4.3.2, `onnxruntime-web` 1.31.0-dev(앱의 `frontend/node_modules`를 그대로 씀).
  - Vite로 묶은 모듈 Worker 안에서 `pipeline('text-to-speech', 모델, {device: 'wasm', dtype})`. ONNX Runtime 파일은 `localTtsWorker.js`처럼 `?url`로 묶어 로컬에서 제공했다(CDN 사용 안 함). 시험 때는 Asyncify 빌드(`ort-wasm-simd-threaded.asyncify.mjs/.wasm`)였고, 앱은 지금 세 Worker 모두 일반 빌드(`ort-wasm-simd-threaded.mjs/.wasm`, `ortWasm.js`)를 쓴다. Asyncify는 WebGPU에만 필요하고, WebKit은 그 빌드를 컴파일하는 데 몇 분과 수 GB 메모리를 쓴다. 이 문서의 Chromium 측정값은 두 빌드에서 거의 같다.
  - `env.allowLocalModels = false`, `env.remoteHost = http://localhost:<port>/api/local-models/`, `env.useBrowserCache = false`. 모델 파일은 백엔드 경로 `/api/local-models/<org>/<name>/resolve/main/<file>`를 흉내 낸 작은 Node 서버가 디스크에서 내줬다(저장소 허용 목록도 흉내 냄). 즉 "모델 준비 시간"은 파일이 이미 백엔드 디스크에 있을 때, 새 브라우저에서 처음 불러오는 시간이다. 인터넷에서 내려받는 시간은 포함하지 않는다.
  - 앱처럼 COOP/COEP 헤더가 없어 `crossOriginIsolated = false`이고, 그래서 WASM은 스레드 1개로 돈다. 비교용으로 헤더를 켠 경우(스레드 2개)도 한 번 쟀다.
- 합성 시간은 Worker 안에서 `tts(text, options)` 호출 전후를 잰 값. RTF = 합성 시간 / 음성 길이.
- 검증: 모든 wav에 대해 길이, RMS, 최대값, 클리핑 샘플 수, 20 ms 단위 음성 구간 비율(-40 dBFS 초과), 영교차율을 계산했다. 그리고 `Xenova/whisper-small`(다국어, encoder fp32 + decoder q8)을 Node의 transformers.js 4.3.2로 돌려 `language: 'korean'`으로 받아썼다(영어가 들어간 파일은 `english`로도). 앱이 쓰는 `Xenova/whisper-base.en`은 영어 전용이라 한국어 확인에 쓸 수 없어 multilingual small을 썼다.
- 시험 코드: `ko-tts-spike/app/worker.js`(Worker), `run-browser.cjs`(Playwright 구동, wav 저장), `server.cjs`(백엔드 경로 흉내), `check.mjs`(검사와 받아쓰기). 측정 원자료는 `results-*.json`, `check-*.json`.

## 후보 정보

| 항목 | `onnx-community/Supertonic-TTS-2-ONNX` | `open-world-group/Supertonic-3-ONNX` |
|---|---|---|
| 원본 | Supertone/supertonic-2 (Supertone Inc.) | Supertonic 3 (README는 `supertone-oss-archive/supertonic-3`를 원본으로 적음. `Supertone/supertonic-3`도 있음) |
| 변환한 곳 | onnx-community(transformers.js 관리 조직) | open-world-group(제3자, 2026-10-24 생성, 내려받기 350회) |
| 라이선스 | OpenRAIL-M(모델 카드 `license: openrail`, 원본 저장소 LICENSE는 BigScience Open RAIL-M). 사용 제한 조항이 있다. 앱 코드 라이선스와는 별개 | OpenRAIL-M로 표기. 하지만 `manifest.json`에 있는 `LICENSE` 파일이 저장소에 없다(404) |
| 저장소 revision | `68d4d9420d0e0e51d14656e1ec5c9b091490b49e` | `0642207fc25f1a390cde0d7a4536de477d372b4a` |
| 지원 언어 | en, ko, es, pt, fr | 31개(ko 포함) |
| 제공 dtype | fp32만(`*_quantized`, `*_fp16` 없음) | fp32, fp16, q8(latent_denoiser만) |
| 출력 | 44.1 kHz 모노 | 44.1 kHz 모노 |
| 음성 | `voices/F1~F5.bin`, `voices/M1~M5.bin`(각 51,712바이트, float32 스타일 벡터) | 같은 이름, 같은 크기 |
| 언어 지정 | 문장을 `<ko>…</ko>`로 감싼다(옵션 아님, 텍스트 태그) | 같음(`<lang>…</lang>`, 모를 땐 `na`) |
| 4.3.2 지원 | 됨(`SupertonicForConditionalGeneration`, `text-to-audio.js`의 `_call_supertonic`) | 됨(같은 코드 경로) |

`Supertone/supertonic-3` 공식 저장소는 transformers.js 형식이 아니다(`duration_predictor/text_encoder/vector_estimator/vocoder` 4개 그래프, `unicode_indexer.json`, `voice_styles/*.json`, `config.json`은 다운로드 집계용 빈 설정). 4.3.2의 pipeline으로는 불러올 수 없어서, transformers.js 형식으로 다시 묶은 `open-world-group/Supertonic-3-ONNX`를 1b로 시험했다. 이 저장소의 "fp32 출력이 공식 4-모델 파이프라인과 정확히 같다"는 README 설명은 검증하지 못했다(그래프를 합쳐서 해시 비교가 안 된다). 그 밖에 `Kanha-AI/supertonic-tts-2-fp16-ONNX`(Supertonic 2의 fp16 판, 제3자)도 있지만, WASM에서는 fp16이 이점이 없어 시험하지 않았다.

### 내려받는 파일과 크기 (Worker가 실제로 요청한 파일, 서버 기록 기준)

Supertonic 2, fp32 — 총 **262,878,761바이트(약 263 MB) + 음성 파일 1개당 51,712바이트**
- `config.json` 247, `tokenizer.json` 5,658, `tokenizer_config.json` 119
- `onnx/text_encoder.onnx` 433,173 + `onnx/text_encoder.onnx_data` 28,530,432
- `onnx/latent_denoiser.onnx` 397,050 + `onnx/latent_denoiser.onnx_data` 132,098,880
- `onnx/voice_decoder.onnx` 59,730 + `onnx/voice_decoder.onnx_data` 101,353,472
- `voices/<음성>.bin` 51,712 (합성할 때 `speaker_embeddings` URL로 받음)

Supertonic 3
- q8 구성(`{text_encoder: 'fp32', latent_denoiser: 'q8', voice_decoder: 'fp32'}`): 총 **207,367,469바이트** + 음성 파일. `onnx/text_encoder.onnx` 40,268,818, `onnx/latent_denoiser_quantized.onnx` 65,477,984, `onnx/voice_decoder.onnx` 101,435,767, `tokenizer.json` 184,532, 설정 파일 2개.
- fp32: 총 **398,365,739바이트**(`latent_denoiser.onnx`가 256,476,254).

## 측정 결과

준비 시간은 새 브라우저에서 `pipeline()` 호출부터 끝날 때까지. 합성은 음성 F1, 5단계, 속도 1.05, 스레드 1개.

| 측정 | Supertonic 2 fp32 | Supertonic 3 q8 | Supertonic 3 fp32 |
|---|---|---|---|
| 모델 준비(콜드, 파일은 로컬 디스크) | 2.49~2.66초 (7회) | 3.22초 | 3.28초 |
| 문장 1 (처음 / 다시) | 1.48초 / 1.33초, 음성 3.03초, RTF 0.44~0.49 | 2.41초 / 2.21초, 음성 3.04초, RTF 0.73~0.79 | 3.26초 / 2.94초, RTF 0.97~1.07 |
| 문장 2 | 1.58초, 음성 3.64초, RTF 0.43 | 2.61초, 3.57초, RTF 0.73 | – |
| 문장 3 (전체 `<ko>`) | 1.58초, 음성 3.80초, RTF 0.42 | 3.05초, 4.54초, RTF 0.67 | – |
| 문장 4 | 1.49초, 음성 3.90초, RTF 0.38 | 2.71초, 4.24초, RTF 0.64 | – |
| 문장 5 (`<en>`) | 1.10초, 음성 2.29초, RTF 0.48 | 1.78초, 2.25초, RTF 0.79 | – |
| 문장 1, 2단계 | 1.05초, RTF 0.35 | 1.30초 (클리핑 72개) | 1.44초 (클리핑 43개) |
| 문장 1, 10단계 | 2.04초, RTF 0.67 | 3.92초, RTF 1.29 | – |
| 긴 문단(118자, 4문장) | 6.88초, 음성 17.4초, RTF 0.40 | – | – |
| 스레드 2개(COOP/COEP 켬) | 문장 1: 0.94초 / 0.78초 | – | – |

- 음성별(문장 1, Supertonic 2): F1 1.27초, F2 1.27초, F3 1.37초, M1 1.33초, M2 1.35초, M3 1.18초. 음성 사이 차이는 작다.
- 콘솔 오류·경고: 두 모델 모두 페이지와 Worker에서 경고나 오류가 없었다.
- Supertonic 3 README는 q8 denoiser가 fp32보다 2.6배 빠르다고 적었지만, 여기서는 약 1.3배였다.

## 받아쓰기 확인 (Whisper small, `language: 'korean'`)

모든 파일에서 클리핑 없음(2단계 Supertonic 3 제외), RMS -23~-30 dBFS, 음성 구간 비율 0.5~0.8, 영교차율 0.03~0.06으로 소음이 아니라 말소리 특성을 보였다. 음성 길이도 글자 수에 맞았다(한국어 약 8~9자/초). 예외는 언어 태그를 구간별로 섞은 파일뿐이다.

| 문장 | Supertonic 2 (F1) | Supertonic 3 q8 (F1) |
|---|---|---|
| 1. 전반적으로 내용 전달은 좋았습니다. | 전반적으로 내용 전달은 좋았습니다. (일치) | 전반적으로 내용 전달은 좋았습니다. (일치) |
| 2. 과거 시제를 쓸 때 동사 형태에 주의하세요. | 일치 | 일치 |
| 3. "I goed …" 대신 "I went …"라고 해야 합니다. (전체 `<ko>`) | I go to the park. I went to the park라고 해야 합니다. ("대신" 빠짐) | I go to the park 대신, I went to the park 라고 해야 합니다. |
| 3. 구간별 태그 `<ko>"</ko><en>…</en>…` | "택 the park" — 길이 2.45초, 음성 구간 16%로 대부분 무음. 실패 | "루 난 툭!" — 음성 구간 15%. 실패 |
| 4. 다음에는 구체적인 예시를 두 가지 이상 들어 보세요. | 다음에는 구체적인 예시를 두 가지 이상 들어보세요. (띄어쓰기만 다름) | 같음 |
| 5. Overall the answer was clear. (`<en>`, 영어로 받아씀) | Overall, the answer was clear. | Overall, the answer was clear. |

문장 3 추가 확인(Supertonic 2):
- F1으로 두 번 합성해 두 번 다 "대신"이 받아쓰기에서 빠졌다. 같은 문장을 F2, M1으로 합성하면 "I go to the park 대신 I went to the park 라고 해야 합니다."로 "대신"이 들어갔다. 음성(F1)에 따라 짧은 한국어 단어가 약하게 발음되는 것으로 보이지만 귀로 확인하지 못했다.
- "goed"는 모든 파일에서 "go"로 받아써졌다. Whisper가 틀린 영어를 고쳐 받아쓰는 경향이 있어서, 모델이 "goed"를 읽었는지는 이 방법으로 알 수 없다. 학습 피드백에서 중요한 부분이므로 직접 들어 봐야 한다.
- 구간을 나눠 따로 합성해 이어 붙인 판(`supertonic2-3-split.wav`, `<en>I goed to the park</en>` / `<ko>대신</ko>` / `<en>I went to the park</en>` / `<ko>라고 해야 합니다.</ko>`, 사이 0.12초)은 영어 받아쓰기에서 "I go to the park. 대신, I went to the park. 라고 해야 합니다."로 내용은 다 들어갔다. 합성 호출이 4번(합계 3.1초)이고 이음새가 부자연스러울 가능성이 크다.
- 태그를 공백으로 띄운 `<en>…</en> <ko>…</ko> …`는 완전 무음(-67 dBFS)이었다(Whisper는 무음에서 "MBC 뉴스 김상현입니다."를 지어냄).

그 밖의 확인(Supertonic 2):
- 태그 없이 한국어만 넣으면 "전반반적으로 내용정 전단다를 좋아했습니다."로 무너진다. **태그는 반드시 넣어야 한다.** (Supertonic 3는 태그 없이도 맞게 받아써졌다.)
- 영어 문장을 `<ko>`로 감싸도 영어로 받아써졌다(`supertonic2-5-as-ko.wav`).
- 2단계는 "내용턴은"으로 한 글자가 틀렸다. 5단계와 10단계는 일치.
- 숫자: "예시를 2가지 이상, 3분 안에 말해 보세요."는 "예시를 두 가지 이상 3분 안에…"로 받아써졌다. "2가지"를 "두 가지"로 읽은 것은 맞는 것 같지만, "3분"은 Whisper가 숫자로 적어서 "삼 분"으로 읽었는지 알 수 없다.
- 4문장 118자 문단 한 번에 합성: 전체가 맞게 받아써졌다(연결어 "사이의"→"사이에" 한 곳만 다름).

## WAV 파일 목록 (`out/`)

모두 16-bit PCM 모노 44,100 Hz. 별도 표시가 없으면 음성 F1, 5단계, 속도 1.05.

Supertonic 2 (`onnx-community/Supertonic-TTS-2-ONNX`, fp32)
- `supertonic2-1.wav`, `-2.wav`, `-4.wav`: 예문 1, 2, 4 (`<ko>`)
- `supertonic2-3-ko.wav`: 예문 3 전체를 `<ko>`로
- `supertonic2-3-mixed.wav`: 예문 3, 구간별 태그(실패 사례)
- `supertonic2-3-mixed-spaced.wav`: 예문 3, 태그 사이 공백(무음, 실패 사례)
- `supertonic2-3-split.wav`: 예문 3을 4구간으로 따로 합성해 이어 붙임
- `supertonic2-5.wav`: 예문 5 (`<en>`), `supertonic2-5-as-ko.wav`: 같은 영어 문장을 `<ko>`로
- `supertonic2-1-voice-F1/F2/F3/M1/M2/M3.wav`: 음성 비교(예문 1)
- `supertonic2-1-steps2.wav`, `supertonic2-1-steps10.wav`: 추론 단계 비교(5단계는 `supertonic2-1.wav`)
- `supertonic2-1-again.wav`: 같은 설정 재합성(잡음 시드가 달라 조금 다름)
- `supertonic2-F2-2/-3-ko/-4/-5.wav`: 추천 음성 F2로 만든 나머지 예문(F2의 예문 1은 `supertonic2-1-voice-F2.wav`)
- `supertonic2-notag.wav`: 태그 없이 한국어(무너지는 사례)
- `supertonic2-x-digits.wav`, `supertonic2-x-digits-spelled.wav`: 숫자 표기와 한글 표기 비교
- `supertonic2-x-long.wav`: 4문장 문단 한 번에

Supertonic 3 (`open-world-group/Supertonic-3-ONNX`)
- `supertonic3-q8-1/-2/-3-ko/-3-mixed/-4/-5/-5-as-ko.wav`: 위와 같은 뜻, q8 구성
- `supertonic3-q8-1-voice-F1/F2/F3/M1/M2/M3.wav`: 음성 비교
- `supertonic3-q8-1-steps2.wav`(클리핑 있음), `supertonic3-q8-1-steps10.wav`, `supertonic3-q8-1-again.wav`
- `supertonic3-q8-notag.wav`: 태그 없이 한국어
- `supertonic3-fp32-1.wav`, `-1-again.wav`, `-1-steps2.wav`: fp32 구성

## 추천과 근거

**`onnx-community/Supertonic-TTS-2-ONNX`, fp32, 5단계, 속도 1.05, 음성 F2(잠정)**

- 앱이 쓰는 라이브러리 4.3.2에 이 모델 종류의 코드가 들어 있어, 추가 패키지 없이 `pipeline('text-to-speech', …)` 한 줄로 돈다. Kokoro처럼 `kokoro-js`나 fetch 우회가 필요 없다.
- 한국어 예문은 받아쓰기가 입력과 같았다. 단일 스레드에서 RTF 약 0.4~0.5라 한 문장(3~4초 분량)을 1.5초 안팎에 만든다. Supertonic 3 q8보다 약 1.6배 빠르다.
- transformers.js 관리 조직이 변환한 저장소라 출처가 분명하다. Supertonic 3 판은 제3자가 합친 그래프이고 LICENSE 파일이 빠져 있다.
- 단점: dtype이 fp32뿐이라 내려받기가 263 MB(Kokoro q8보다 훨씬 큼)이고 메모리도 그만큼 쓴다(메모리 사용량은 재지 않았다).
- 음성: F1은 예문 3에서 "대신"이 두 번 다 받아써지지 않았고, F2와 M1은 모든 시험에서 받아쓰기가 맞았다. 그래서 F2를 잠정 기본으로 둔다. 실제 선택은 `supertonic2-1-voice-*.wav`를 들어 보고 정한다.
- 단계 수: 2단계는 빠르지만 한 글자가 틀렸고, 10단계는 시간이 1.5배다. 5단계를 기본으로 둔다.

## 통합에 필요한 값

```
KO_TTS_MODEL   = 'onnx-community/Supertonic-TTS-2-ONNX'
KO_TTS_DTYPE   = 'fp32'          // 이 저장소에는 fp32만 있다. 'q8'을 주면 *_quantized.onnx를 찾다 404가 난다.
KO_TTS_VOICE   = 'F2'            // 잠정. F1~F5, M1~M5 중 하나. 파일은 voices/<id>.bin
KO_TTS_OPTIONS = {num_inference_steps: 5, speed: 1.05}
```

호출 방법(Worker 안):

```js
const tts = await pipeline('text-to-speech', KO_TTS_MODEL, {device: 'wasm', dtype: KO_TTS_DTYPE})
const voiceUrl = `${modelHost}${KO_TTS_MODEL}/resolve/main/voices/${KO_TTS_VOICE}.bin`
const out = await tts(`<ko>${sentence}</ko>`, {speaker_embeddings: voiceUrl, ...KO_TTS_OPTIONS})
// out.audio: Float32Array, out.sampling_rate: 44100
```

- 언어 지정: 옵션이 아니라 텍스트를 `<ko>…</ko>`로 감싼다(영어는 `<en>…</en>`). 토크나이저가 글자 단위라 태그도 글자로 들어가고, 모델이 그 글자열을 언어 표시로 배웠다. 태그를 빠뜨리면 한국어가 무너진다. 한 입력 안에서 태그를 여러 번 섞으면 안 된다. 영어 인용이 섞인 한국어 문장은 전체를 `<ko>`로 감싼다.
- 음성 파일: `speaker_embeddings`에 문자열 URL을 주면 라이브러리가 `env.fetch`로 직접 받는다(`env.remoteHost`를 거치지 않는다). 그래서 위처럼 백엔드 주소를 넣은 전체 URL을 줘야 한다. Float32Array를 직접 줘도 된다. 매 호출마다 다시 받으므로 Worker에서 한 번 받아 Float32Array로 갖고 있는 편이 낫다(51,712바이트).
- 백엔드 허용 목록: `onnx-community/Supertonic-TTS-2-ONNX` 한 저장소면 된다. 모델·토크나이저·음성 파일이 모두 이 안에 있다. 기본 목록(`DEFAULT_LOCAL_MODEL_REPOS`, `backend/crates/infrastructure/src/config.rs`)에 추가해야 한다. 파일 이름은 모두 백엔드 경로 규칙(`[A-Za-z0-9._-]`)에 맞는다. `.onnx_data`는 `application/octet-stream`으로 나가며 문제없었다.
- 파일을 백엔드 디스크에 두는 방식(`env.useBrowserCache = false`)으로 그대로 됐다. 백엔드가 처음 내려받을 때 Hugging Face가 파일을 `us.aws.cdn.hf.co`로 리다이렉트했다(이 시험 환경에서는 그 호스트를 따로 허용해야 했다). 사용자 네트워크에서 이 호스트가 막혀 있으면 내려받기가 실패한다.
- 문장 길이: `model_max_length` 1000에 `truncation: true`라 1000자(NFKD 분해 후 글자 수, 한글은 한 글자가 2~3자로 늘어남)를 넘으면 조용히 잘린다. 문장 단위로 나눠 합성하는 지금 방식(`splitForSpeech`)이면 문제없다. 118자 문단도 한 번에 됐다.
- 텍스트 정규화: 토크나이저가 NFKD 분해, 이모지 제거, 따옴표·대시 통일, `@`→" at ", `e.g.,`→"for example, " 등을 하고, 허용 글자(공백, 기본 문장부호, 숫자, 영문, 한글 자모, 일부 기호) 밖의 글자는 지운다. 숫자는 남지만 한국어로 어떻게 읽는지는 확인하지 못했다. 피드백 문장에 숫자가 많다면 한글로 풀어 쓰는 것이 안전하다.
- 첫 합성은 이후보다 약 10% 느리다(1.48초 → 1.33초). 준비 직후 짧은 문장을 한 번 합성해 두면 첫 재생이 빨라진다.
- 출력은 44.1 kHz다. Kokoro(24 kHz)와 다르지만 `localSpeech.js`의 `floatToWavBytes(samples, sampleRate)`는 샘플레이트를 받으므로 그대로 쓸 수 있다.
- 결과 길이는 텍스트 인코더가 예측한 길이로 잘리며(`durations`), 앞뒤 무음이 짧다. 문장을 이어 재생할 때 사이 간격은 앱이 넣어야 할 수 있다.

Supertonic 3를 고를 경우: `KO_TTS_MODEL = 'open-world-group/Supertonic-3-ONNX'`, `KO_TTS_DTYPE = {text_encoder: 'fp32', latent_denoiser: 'q8', voice_decoder: 'fp32'}`, 같은 음성 이름, 같은 옵션, 같은 태그 방식. 허용 목록에 `open-world-group/Supertonic-3-ONNX`. 2단계는 클리핑이 생기므로 쓰지 않는다. README는 한국어를 120자 이하로 나누라고 한다. LICENSE 파일이 없는 점과 제3자 변환이라는 점은 따로 판단이 필요하다.

## 확인하지 못한 것

- 자연스러움, 억양, 음색: 소리를 듣지 못했다. 받아쓰기 일치는 알아들을 수 있다는 간접 증거일 뿐이다.
- "goed" 같은 틀린 영어를 그대로 읽는지: Whisper가 고쳐 받아써서 알 수 없다.
- 숫자를 한국어로 어떻게 읽는지("3분").
- 사용자 기기에서의 속도: 이 컨테이너(CPU 4개, 단일 스레드 실행)의 값이다. 노트북에서는 다를 수 있다.
- 메모리 사용량: 재지 않았다. fp32 263 MB 모델이 WASM 메모리에 올라가므로, Whisper와 Kokoro가 함께 떠 있을 때 메모리가 부족한지 확인이 필요하다.
- WKWebView(macOS)와 WebView2(Windows)에서의 동작: Chromium에서만 시험했다.
- 인터넷에서 처음 내려받는 시간: 측정하지 않았다(263 MB).
- `Xenova/mms-tts-kor`: 시험하지 않았다.
