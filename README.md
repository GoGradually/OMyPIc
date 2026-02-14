# OMyPIc 데스크톱 앱 실행/패키징 안내

이 문서는 OMyPIc를 Electron 기반 단일 실행 앱으로 개발/배포하는 방법을 설명합니다.

## 1) 구성

- Backend: Spring Boot (`backend/`)
- Frontend: React + Vite (`frontend/`)
- Desktop: Electron (`frontend/electron/`)
- 번들 리소스: Backend JAR + MongoDB + JRE17 (`frontend/electron/resources/`)

## 2) 개발 실행

개발 모드에서는 Electron이 Vite dev 서버를 띄우며, Backend/MongoDB는 외부 실행을 기본으로 가정합니다.

```bash
# terminal 1
cd backend
./gradlew bootRun

# terminal 2
cd frontend
npm install
npm run dev
```

- Backend 기본 포트: `4317`
- Vite dev 서버: `http://localhost:5173`

## 3) 배포 패키징

`electron:prep` 단계에서 아래 작업이 자동 수행됩니다.

1. Backend `:bootstrap:bootJar` 빌드
2. JAR를 `electron/resources/backend/omypic-backend.jar`로 복사
3. 현재 OS/아키텍처용 MongoDB 바이너리 다운로드 및 sha256 검증
4. 현재 OS/아키텍처용 Temurin JRE17 다운로드 및 sha256 검증
5. 리소스를 `electron/resources`로 정리

```bash
cd frontend
npm install

# 공통 빌드 (현재 플랫폼 기준)
npm run electron:build

# Windows 설치형 (NSIS)
npm run electron:build:win

# macOS 설치형 (DMG)
npm run electron:build:mac
```

산출물은 `frontend/dist/` 하위에 생성됩니다.
Windows 설치형은 Windows 환경에서, macOS DMG는 macOS 환경에서 빌드하는 것을 권장합니다.

## 4) 런타임 기동 순서

패키지된 앱 실행 시 Electron 메인 프로세스는 아래 순서로 동작합니다.

1. 번들된 `mongod` 실행
2. 포트 `27017` 준비 대기
3. 번들된 JRE로 Backend JAR 실행
4. `/actuator/health` 준비 대기
5. 프론트 UI 로드

## 5) 환경 변수

### Electron/Backend 공통

- `OMYPIC_BACKEND_URL`
  - 기본값: `http://localhost:4317`
- `OMYPIC_MONGODB_URI`
  - 기본값: `mongodb://127.0.0.1:27017/omypic`
- `OMYPIC_DATA_DIR`
  - 기본값: Electron userData 하위 `omypic-data`

### 기동 Override

- `OMYPIC_BACKEND_CMD`
  - 지정 시 해당 명령으로 Backend 실행
- `OMYPIC_MONGODB_BIN`
  - 지정 시 해당 `mongod` 경로 실행
- `OMYPIC_BACKEND_URL` 또는 `OMYPIC_MONGODB_URI`를 외부 서버로 지정하면 로컬 번들 Backend/MongoDB를 기동하지 않고 외부 서버를 사용합니다.

## 6) 코드서명 정책

- 현재 빌드는 개발/사내 배포용 unsigned 아티팩트를 기준으로 합니다.
- Windows 코드서명, macOS notarization은 후속 단계에서 적용합니다.

## 7) API Key 저장

- API Key는 Electron에서 OS Keychain(`keytar`)에 저장됩니다.
- Backend에는 저장하지 않고 요청 시 헤더로 전달됩니다.
- 현재 앱은 OpenAI API Key만 지원합니다.
