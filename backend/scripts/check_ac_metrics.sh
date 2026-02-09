#!/usr/bin/env bash
set -euo pipefail

BASE_URL=${1:-http://localhost:4317}

extract_value() {
  local payload=$1
  python3 - "$payload" <<'PY'
import json
import sys

raw = sys.argv[1].strip()
if not raw:
    print("")
    raise SystemExit

try:
    data = json.loads(raw)
except Exception:
    print("")
    raise SystemExit

values = []
for measurement in data.get("measurements", []):
    statistic = str(measurement.get("statistic", "")).upper()
    if statistic == "VALUE":
        value = measurement.get("value")
        if isinstance(value, (int, float)):
            values.append(value)

if values:
    print(values[0])
else:
    print("")
PY
}

fetch_p95() {
  local metric=$1
  local payload
  local value

  payload=$(curl -fsS "${BASE_URL}/actuator/metrics/${metric}?tag=quantile:0.95" || true)
  value=$(extract_value "$payload")

  if [[ -z "$value" ]]; then
    payload=$(curl -fsS "${BASE_URL}/actuator/metrics/${metric}" || true)
    value=$(extract_value "$payload")
  fi

  echo "$value"
}

is_within_threshold() {
  local value=$1
  local threshold=$2
  python3 - "$value" "$threshold" <<'PY'
import sys

value = float(sys.argv[1])
threshold = float(sys.argv[2])
print("1" if value <= threshold else "0")
PY
}

check_metric() {
  local metric=$1
  local threshold=$2
  local label=$3
  local value
  value=$(fetch_p95 "$metric")

  if [[ -z "$value" ]]; then
    echo "[FAIL] ${label}: p95 값을 찾지 못했습니다. metric=${metric}"
    return 1
  fi

  local pass
  pass=$(is_within_threshold "$value" "$threshold")
  if [[ "$pass" == "1" ]]; then
    echo "[OK]   ${label}: p95=${value}s <= ${threshold}s"
    return 0
  fi

  echo "[FAIL] ${label}: p95=${value}s > ${threshold}s"
  return 1
}

failed=0

check_metric "feedback.latency" "10.0" "즉시 피드백 표시" || failed=1
check_metric "question.next.latency" "0.5" "다음 질문 전환" || failed=1
check_metric "rulebook.upload.latency" "3.0" "룰북 업로드 후 목록 반영" || failed=1

if [[ "$failed" -ne 0 ]]; then
  echo "AC p95 점검 실패"
  exit 1
fi

echo "AC p95 점검 통과"
