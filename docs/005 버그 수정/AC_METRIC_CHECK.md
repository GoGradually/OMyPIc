# AC p95 자동 점검 절차

## 목적

SRS의 성능 AC를 배포 전/회귀 테스트 시 자동으로 판정한다.

## 대상 AC

- `feedback.latency` p95 <= `10.0s` (즉시 피드백 표시)
- `question.next.latency` p95 <= `0.5s` (다음 질문 전환)
- `rulebook.upload.latency` p95 <= `3.0s` (업로드 후 목록 반영)

## 사전 조건

- 백엔드가 실행 중이어야 한다.
- Actuator metrics 엔드포인트가 노출되어야 한다 (`/actuator/metrics`).
- 테스트 트래픽을 먼저 충분히 발생시켜 타이머 샘플을 확보해야 한다.

## 실행

레포 루트에서 실행:

```bash
./backend/scripts/check_ac_metrics.sh
```

다른 호스트/포트 사용 시:

```bash
./backend/scripts/check_ac_metrics.sh http://localhost:4317
```

## 판정

- 모든 항목 `OK`이면 통과
- 하나라도 `FAIL`이면 실패
- metric 샘플이 없거나 p95 값을 찾을 수 없으면 실패로 처리

## 운영 팁

- CI에서는 성능 시나리오 실행 후 본 스크립트를 붙여 회귀 감지에 사용한다.
- 실패 시 해당 metric의 raw 값(`GET /actuator/metrics/{name}`)을 함께 수집해 원인 분석한다.
