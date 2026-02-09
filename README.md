# OMyPIc 로컬 앱 실행/패키징 안내

이 문서는 OMyPIc 로컬 데스크톱 앱을 **개발 실행**하고 **패키징**하는 방법을 설명합니다.

## 1) 구성 요약

- **Backend**: Spring Boot (`backend/`)
- **Frontend**: React + Vite (`frontend/`)
- **Desktop**: Electron (`frontend/electron/`)
- **DB**: 로컬 MongoDB (바이너리 번들 방식)

### 1-1. Backend 패키지 구조 (애그리거트 중심)

- 4개 레이어 모듈: `domain`, `application`, `presentation`, `infrastructure`
- 애그리거트: `rulebook`, `question`, `session`, `feedback`, `wrongnote`, `stt`, `tts`
- 공통 코드: 각 레이어의 `shared` 패키지로 분리
- 예시 경로
    - `backend/domain/src/main/java/me/go_gradually/omypic/domain/question`
    - `backend/application/src/main/java/me/go_gradually/omypic/application/feedback/usecase`
    - `backend/presentation/src/main/java/me/go_gradually/omypic/presentation/stt/controller`
    - `backend/infrastructure/src/main/java/me/go_gradually/omypic/infrastructure/rulebook/persistence/mongo`

---

## 2) 개발 실행 (로컬)

### 2-1. Backend 실행

```bash
cd backend
./gradlew bootRun
```

- 기본 포트: `4317`

### 2-2. Frontend + Electron 실행

```bash
cd frontend
npm install
npm run dev
```

- Vite dev 서버: `http://localhost:5173`
- Electron에서 해당 주소를 로드합니다.

---

## 3) 환경 변수

### Backend

- `OMYPIC_MONGODB_URI`
    - 기본값: `mongodb://localhost:27017/omypic`
- `OMYPIC_DATA_DIR`
    - 기본값: `${user.home}/OMyPIc`

### Electron

- `OMYPIC_BACKEND_URL`
    - 기본값: `http://localhost:4317`
- `OMYPIC_BACKEND_CMD`
    - 지정 시 해당 커맨드를 실행해 백엔드를 실행
- `OMYPIC_MONGODB_BIN`
    - 지정 시 해당 경로의 `mongod` 실행

---

## 4) 패키징(배포용 데스크톱 앱)

### 4-1. Backend JAR 준비

```bash
cd backend
./gradlew clean build
```

- 빌드 결과: `backend/bootstrap/build/libs/*.jar`
- 이 파일을 **Electron resources**에 복사합니다.
    - 예: `frontend/electron/backend/omypic-backend.jar`

### 4-2. MongoDB 바이너리 번들

- MongoDB 바이너리를 Electron resources에 포함해야 합니다.
- 예상 경로:
    - `frontend/electron/mongodb/bin/mongod`

> 참고: OS별 바이너리 분리 패키징이 필요합니다. (macOS/Windows)

### 4-3. Electron 패키징

```bash
cd frontend
npm install
npm run electron:build
```

- 생성된 설치 파일은 `frontend/dist` 또는 `frontend/dist_electron` 등에 생성됩니다.

---

## 5) 실행 흐름

- Electron 앱 시작 시:
    1. MongoDB 실행 (`mongodb/bin/mongod`)
    2. Backend 실행 (`backend/bootstrap/build/libs/omypic-backend.jar`)
    3. 프론트 UI 로드

---

## 6) 자주 발생하는 문제

### MongoDB 실행 실패

- 바이너리 경로가 잘못됐거나 실행 권한이 없는 경우
- 해결:
    - `OMYPIC_MONGODB_BIN` 환경변수로 경로 지정
    - 실행 권한 부여(`chmod +x mongod`)

### Backend 실행 실패

- JAR 파일 누락 또는 Java 런타임 미설치
- 해결:
    - `frontend/electron/backend/omypic-backend.jar` 위치 확인
    - JRE 번들 또는 시스템 Java 설치

---

## 7) API Key 저장

- API Key는 Electron에서 **OS Keychain**(keytar)으로 저장됩니다.
- Backend는 API Key를 저장하지 않으며, 요청마다 헤더로 전달됩니다.
