# STT 실패율(일 기준) 산출 및 경고 기준

## 지표 정의

- 요청 수: `stt.requests` (카운터)
- 실패 수: `stt.errors` (카운터)
- 실패율(%) = `(일간 stt.errors 증가량 / 일간 stt.requests 증가량) * 100`

`stt.requests`는 STT 유효 요청이 처리될 때 1 증가한다.

## 일간 집계 규칙

- 집계 경계: 로컬 시간 기준 `00:00 ~ 23:59:59`
- 계산 방식: 각 카운터의 "당일 시작값"과 "현재값" 차이
- 분모(`stt.requests`)가 0이면 실패율 계산은 `N/A`로 처리

## 경고 기준

- 기본 경고(Warning): 실패율 `>= 2.0%`
- 심각(Critical): 실패율 `>= 5.0%`

## 조회 예시

```bash
curl -sS http://localhost:4317/actuator/metrics/stt.requests
curl -sS http://localhost:4317/actuator/metrics/stt.errors
```

## 운영 권고

- Warning 발생 시: 최근 배포/외부 STT API 장애/네트워크 상태를 점검한다.
- Critical 발생 시: 즉시 알림 채널 전파 및 재시도 정책/요청 크기 분포를 확인한다.
