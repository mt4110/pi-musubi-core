#!/usr/bin/env bash
set -euo pipefail

new_smoke_run_id() {
  if command -v uuidgen >/dev/null 2>&1; then
    uuidgen | tr '[:upper:]' '[:lower:]'
  else
    printf '%s-%s' "$(date -u +%Y%m%d%H%M%S)" "$$"
  fi
}

ENV_FILE="${ENV_FILE:-.env}"
LOAD_ENV_FILE="${HTTP_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE:-true}"
HOST="${HTTP_HAPPY_ROUTE_SMOKE_HOST:-127.0.0.1}"
PORT="${HTTP_HAPPY_ROUTE_SMOKE_PORT:-18089}"
BASE_URL="${HTTP_HAPPY_ROUTE_SMOKE_BASE_URL:-http://${HOST}:${PORT}}"
STARTUP_TIMEOUT_SECONDS="${HTTP_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS:-120}"
SMOKE_RUN_ID="${HTTP_HAPPY_ROUTE_SMOKE_RUN_ID:-$(new_smoke_run_id)}"
INITIATOR_PI_UID="${HTTP_HAPPY_ROUTE_SMOKE_INITIATOR_PI_UID:-http-pilot-smoke-initiator}"
COUNTERPARTY_PI_UID="${HTTP_HAPPY_ROUTE_SMOKE_COUNTERPARTY_PI_UID:-http-pilot-smoke-counterparty}"
REALM_ID="${HTTP_HAPPY_ROUTE_SMOKE_REALM_ID:-realm-http-pilot-smoke}"
PROMISE_IDEMPOTENCY_KEY="${HTTP_HAPPY_ROUTE_SMOKE_PROMISE_IDEMPOTENCY_KEY:-http-pilot-smoke-promise-${SMOKE_RUN_ID}}"
DEPOSIT_MINOR_UNITS="${HTTP_HAPPY_ROUTE_SMOKE_DEPOSIT_MINOR_UNITS:-10000}"
CURRENCY_CODE="${HTTP_HAPPY_ROUTE_SMOKE_CURRENCY_CODE:-PI}"

command -v curl >/dev/null 2>&1 || { echo "curl is required for http-happy-route-smoke"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq is required for http-happy-route-smoke"; exit 1; }

backend_log="$(mktemp -t musubi-backend-http-happy-route-log.XXXXXX)"
body_file="$(mktemp -t musubi-backend-http-happy-route-body.XXXXXX)"
backend_pid=""
dotenv_skip_flag=false

cleanup() {
  status=$?
  if [ -n "$backend_pid" ] && kill -0 "$backend_pid" 2>/dev/null; then
    kill "$backend_pid" >/dev/null 2>&1 || true
    wait "$backend_pid" 2>/dev/null || true
  fi
  if [ "$status" -ne 0 ]; then
    echo "HTTP happy-route smoke failed; last response body:" >&2
    cat "$body_file" >&2 || true
    echo "HTTP happy-route smoke failed; backend log (tail):" >&2
    tail -n 200 "$backend_log" >&2 || true
  fi
  rm -f "$backend_log" "$body_file"
  exit "$status"
}
trap cleanup EXIT INT TERM

case "$LOAD_ENV_FILE" in
  true)
    set -a
    . "./${ENV_FILE}"
    set +a
    ;;
  false)
    dotenv_skip_flag=true
    ;;
  *)
    echo "HTTP_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE must be true or false"
    exit 1
    ;;
esac

cargo build -p musubi_backend

if curl -fsS --max-time 2 "${BASE_URL}/health" > "$body_file" 2>/dev/null; then
  echo "HTTP happy-route smoke port already has a responding /health endpoint: ${BASE_URL}"
  echo "stop the existing process or set HTTP_HAPPY_ROUTE_SMOKE_PORT to a free port"
  exit 1
fi

echo "starting backend for HTTP happy-route smoke at ${BASE_URL}"
APP_HOST="$HOST" \
PORT="$PORT" \
MUSUBI_SKIP_DOTENV="$dotenv_skip_flag" \
MUSUBI_LAUNCH_MODE=pilot \
MUSUBI_LAUNCH_ALLOWLIST_PI_UIDS="${INITIATOR_PI_UID},${COUNTERPARTY_PI_UID}" \
MUSUBI_KILL_SWITCH_AUTH=false \
MUSUBI_KILL_SWITCH_PROMISE_CREATION=false \
./target/debug/musubi_backend > "$backend_log" 2>&1 &
backend_pid=$!

ready=0
for _attempt in $(seq 1 "$STARTUP_TIMEOUT_SECONDS"); do
  if ! kill -0 "$backend_pid" 2>/dev/null; then
    echo "backend exited before HTTP happy-route smoke became ready"
    cat "$backend_log"
    exit 1
  fi
  if curl -fsS --max-time 2 "${BASE_URL}/health" > "$body_file" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 1
done

if [ "$ready" != "1" ]; then
  echo "backend did not become ready within ${STARTUP_TIMEOUT_SECONDS}s"
  cat "$backend_log"
  exit 1
fi
if ! kill -0 "$backend_pid" 2>/dev/null; then
  echo "backend exited after readiness probe succeeded"
  cat "$backend_log"
  exit 1
fi
jq -e '.status == "ok"' "$body_file" >/dev/null

curl -fsS --max-time 5 "${BASE_URL}/api/launch/posture" > "$body_file"
jq -e '.launch_mode == "pilot" and .participant_posture == "pilot_only" and .message_code == "launch_pilot_not_allowed"' "$body_file" >/dev/null

initiator_payload="$(jq -cn \
  --arg pi_uid "$INITIATOR_PI_UID" \
  --arg username "$INITIATOR_PI_UID" \
  --arg wallet_address "wallet-${INITIATOR_PI_UID}" \
  --arg access_token "access-token-${INITIATOR_PI_UID}" \
  '{pi_uid:$pi_uid, username:$username, wallet_address:$wallet_address, access_token:$access_token}')"
curl -fsS --max-time 5 \
  -H "content-type: application/json" \
  --data "$initiator_payload" \
  "${BASE_URL}/api/auth/pi" > "$body_file"
initiator_token="$(jq -er '.token' "$body_file")"
jq -e --arg pi_uid "$INITIATOR_PI_UID" '.user.pi_uid == $pi_uid and (.user.id | type == "string" and length > 0)' "$body_file" >/dev/null

counterparty_payload="$(jq -cn \
  --arg pi_uid "$COUNTERPARTY_PI_UID" \
  --arg username "$COUNTERPARTY_PI_UID" \
  --arg wallet_address "wallet-${COUNTERPARTY_PI_UID}" \
  --arg access_token "access-token-${COUNTERPARTY_PI_UID}" \
  '{pi_uid:$pi_uid, username:$username, wallet_address:$wallet_address, access_token:$access_token}')"
curl -fsS --max-time 5 \
  -H "content-type: application/json" \
  --data "$counterparty_payload" \
  "${BASE_URL}/api/auth/pi" > "$body_file"
counterparty_account_id="$(jq -er '.user.id' "$body_file")"
jq -e --arg pi_uid "$COUNTERPARTY_PI_UID" '.user.pi_uid == $pi_uid and (.token | type == "string" and length > 0)' "$body_file" >/dev/null

promise_payload="$(jq -cn \
  --arg key "$PROMISE_IDEMPOTENCY_KEY" \
  --arg realm_id "$REALM_ID" \
  --arg counterparty_account_id "$counterparty_account_id" \
  --argjson deposit_amount_minor_units "$DEPOSIT_MINOR_UNITS" \
  --arg currency_code "$CURRENCY_CODE" \
  '{internal_idempotency_key:$key, realm_id:$realm_id, counterparty_account_id:$counterparty_account_id, deposit_amount_minor_units:$deposit_amount_minor_units, currency_code:$currency_code}')"
curl -fsS --max-time 5 \
  -H "authorization: Bearer ${initiator_token}" \
  -H "content-type: application/json" \
  --data "$promise_payload" \
  "${BASE_URL}/api/promise/intents" > "$body_file"
jq -e '
  .case_status == "pending_funding"
  and (.promise_intent_id | type == "string" and length > 0)
  and (.settlement_case_id | type == "string" and length > 0)
  and .replayed_intent == false
  and (.outbox_event_ids | type == "array" and length > 0)
' "$body_file" >/dev/null

echo "HTTP happy-route smoke passed at ${BASE_URL}"
