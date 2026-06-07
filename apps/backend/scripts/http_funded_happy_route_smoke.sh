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
LOAD_ENV_FILE="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE:-true}"
HOST="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_HOST:-127.0.0.1}"
PORT="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT:-18090}"
BASE_URL="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_BASE_URL:-http://${HOST}:${PORT}}"
STARTUP_TIMEOUT_SECONDS="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_STARTUP_TIMEOUT_SECONDS:-120}"
SMOKE_RUN_ID="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_RUN_ID:-$(new_smoke_run_id)}"
INITIATOR_PI_UID="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_INITIATOR_PI_UID:-http-funded-smoke-initiator}"
COUNTERPARTY_PI_UID="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_COUNTERPARTY_PI_UID:-http-funded-smoke-counterparty}"
REALM_ID="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_REALM_ID:-realm-http-funded-smoke}"
PROMISE_IDEMPOTENCY_KEY="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PROMISE_IDEMPOTENCY_KEY:-http-funded-smoke-promise-${SMOKE_RUN_ID}}"
DEPOSIT_MINOR_UNITS="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_DEPOSIT_MINOR_UNITS:-10000}"
CURRENCY_CODE="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_CURRENCY_CODE:-PI}"
TXID="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_TXID:-pi-tx-funded-${SMOKE_RUN_ID}}"
VERIFY_REPLAY="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_VERIFY_REPLAY:-false}"
VERIFY_DUPLICATE_RECEIPT="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_VERIFY_DUPLICATE_RECEIPT:-false}"
DUPLICATE_TXID="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_DUPLICATE_TXID:-pi-tx-duplicate-${SMOKE_RUN_ID}}"
SMOKE_LOCK_DIR="${HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOCK_DIR:-${TMPDIR:-/tmp}/musubi-http-funded-happy-route-smoke.lock}"

command -v curl >/dev/null 2>&1 || { echo "curl is required for http-funded-happy-route-smoke"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq is required for http-funded-happy-route-smoke"; exit 1; }

backend_log=""
body_file=""
backend_pid=""
dotenv_skip_flag=false

cleanup() {
  status=$?
  trap - EXIT INT TERM
  if [ -n "$backend_pid" ] && kill -0 "$backend_pid" 2>/dev/null; then
    kill "$backend_pid" >/dev/null 2>&1 || true
    wait "$backend_pid" 2>/dev/null || true
  fi
  if [ "$status" -ne 0 ]; then
    echo "HTTP funded happy-route smoke failed; last response body:" >&2
    if [ -n "$body_file" ] && [ -f "$body_file" ]; then
      cat "$body_file" >&2 || true
    else
      echo "(no response body captured)" >&2
    fi
    echo "HTTP funded happy-route smoke failed; backend log (tail):" >&2
    if [ -n "$backend_log" ] && [ -f "$backend_log" ]; then
      tail -n 200 "$backend_log" >&2 || true
    else
      echo "(no backend log captured)" >&2
    fi
  fi
  rmdir "$SMOKE_LOCK_DIR" >/dev/null 2>&1 || true
  if [ -n "$backend_log" ]; then
    rm -f "$backend_log"
  fi
  if [ -n "$body_file" ]; then
    rm -f "$body_file"
  fi
  exit "$status"
}

if ! mkdir "$SMOKE_LOCK_DIR" 2>/dev/null; then
  echo "another HTTP funded happy-route smoke is already running; run funded smoke targets one at a time"
  exit 1
fi
trap cleanup EXIT INT TERM

backend_log="$(mktemp -t musubi-backend-http-funded-happy-route-log.XXXXXX)"
body_file="$(mktemp -t musubi-backend-http-funded-happy-route-body.XXXXXX)"

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
    echo "HTTP_FUNDED_HAPPY_ROUTE_SMOKE_LOAD_ENV_FILE must be true or false"
    exit 1
    ;;
esac

cargo build -p musubi_backend

if curl -fsS --max-time 2 "${BASE_URL}/health" > "$body_file" 2>/dev/null; then
  echo "HTTP funded happy-route smoke port already has a responding /health endpoint: ${BASE_URL}"
  echo "stop the existing process or set HTTP_FUNDED_HAPPY_ROUTE_SMOKE_PORT to a free port"
  exit 1
fi

echo "starting backend for HTTP funded happy-route smoke at ${BASE_URL}"
APP_HOST="$HOST" \
PORT="$PORT" \
MUSUBI_SKIP_DOTENV="$dotenv_skip_flag" \
PROVIDER_MODE=sandbox \
MUSUBI_LAUNCH_MODE=pilot \
MUSUBI_LAUNCH_ALLOWLIST_PI_UIDS="${INITIATOR_PI_UID},${COUNTERPARTY_PI_UID}" \
MUSUBI_KILL_SWITCH_AUTH=false \
MUSUBI_KILL_SWITCH_PROMISE_CREATION=false \
./target/debug/musubi_backend > "$backend_log" 2>&1 &
backend_pid=$!

ready=0
for _attempt in $(seq 1 "$STARTUP_TIMEOUT_SECONDS"); do
  if ! kill -0 "$backend_pid" 2>/dev/null; then
    echo "backend exited before HTTP funded happy-route smoke became ready"
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
settlement_case_id="$(jq -er '.settlement_case_id' "$body_file")"

curl -fsS --max-time 10 \
  -H "content-type: application/json" \
  --data '{}' \
  "${BASE_URL}/api/internal/orchestration/drain" > "$body_file"
payment_id="$(jq -er --arg settlement_case_id "$settlement_case_id" '
  .processed_messages
  | map(select(
      .event_type == "OPEN_HOLD_INTENT"
      and .aggregate_id == $settlement_case_id
      and (.provider_submission_id | type == "string" and length > 0)
    ))
  | first.provider_submission_id
' "$body_file")"

curl -fsS --max-time 5 \
  -H "authorization: Bearer ${initiator_token}" \
  "${BASE_URL}/api/projection/settlement-views/${settlement_case_id}" > "$body_file"
jq -e '
  .current_settlement_status == "pending_funding"
  and .total_funded_minor_units == 0
  and .latest_journal_entry_id == null
' "$body_file" >/dev/null

callback_payload="$(jq -cn \
  --arg payment_id "$payment_id" \
  --arg payer_pi_uid "$INITIATOR_PI_UID" \
  --argjson amount_minor_units "$DEPOSIT_MINOR_UNITS" \
  --arg currency_code "$CURRENCY_CODE" \
  --arg txid "$TXID" \
  '{payment_id:$payment_id, payer_pi_uid:$payer_pi_uid, amount_minor_units:$amount_minor_units, currency_code:$currency_code, txid:$txid, status:"completed"}')"
curl -fsS --max-time 5 \
  -H "content-type: application/json" \
  --data "$callback_payload" \
  "${BASE_URL}/api/payment/callback" > "$body_file"
jq -e '
  (.raw_callback_id | type == "string" and length > 0)
  and .duplicate_callback == false
  and (.outbox_event_ids | type == "array" and length == 1)
' "$body_file" >/dev/null

curl -fsS --max-time 10 \
  -H "content-type: application/json" \
  --data '{}' \
  "${BASE_URL}/api/internal/orchestration/drain" > "$body_file"
jq -e --arg payment_id "$payment_id" '
  .processed_messages
  | any(.event_type == "INGEST_PROVIDER_CALLBACK" and .provider_submission_id == $payment_id)
' "$body_file" >/dev/null

curl -fsS --max-time 5 \
  -H "authorization: Bearer ${initiator_token}" \
  "${BASE_URL}/api/projection/settlement-views/${settlement_case_id}" > "$body_file"
jq -e --argjson deposit_amount_minor_units "$DEPOSIT_MINOR_UNITS" --arg currency_code "$CURRENCY_CODE" '
  .current_settlement_status == "funded"
  and .total_funded_minor_units == $deposit_amount_minor_units
  and .currency_code == $currency_code
  and (.latest_journal_entry_id | type == "string" and length > 0)
' "$body_file" >/dev/null
funded_journal_entry_id="$(jq -er '.latest_journal_entry_id' "$body_file")"

if [ "$VERIFY_REPLAY" = "true" ]; then
  curl -fsS --max-time 5 \
    -H "content-type: application/json" \
    --data "$callback_payload" \
    "${BASE_URL}/api/payment/callback" > "$body_file"
  jq -e '
    (.raw_callback_id | type == "string" and length > 0)
    and .duplicate_callback == true
    and (.outbox_event_ids | type == "array" and length == 1)
  ' "$body_file" >/dev/null

  curl -fsS --max-time 10 \
    -H "content-type: application/json" \
    --data '{}' \
    "${BASE_URL}/api/internal/orchestration/drain" > "$body_file"
  jq -e --arg payment_id "$payment_id" '
    .processed_messages
    | any(
        .event_type == "INGEST_PROVIDER_CALLBACK"
        and .provider_submission_id == $payment_id
        and .already_processed == true
      )
  ' "$body_file" >/dev/null

  curl -fsS --max-time 5 \
    -H "authorization: Bearer ${initiator_token}" \
    "${BASE_URL}/api/projection/settlement-views/${settlement_case_id}" > "$body_file"
  jq -e \
    --argjson deposit_amount_minor_units "$DEPOSIT_MINOR_UNITS" \
    --arg currency_code "$CURRENCY_CODE" \
    --arg funded_journal_entry_id "$funded_journal_entry_id" '
    .current_settlement_status == "funded"
    and .total_funded_minor_units == $deposit_amount_minor_units
    and .currency_code == $currency_code
    and .latest_journal_entry_id == $funded_journal_entry_id
  ' "$body_file" >/dev/null

  echo "HTTP funded replay smoke passed at ${BASE_URL}"
  exit 0
fi

if [ "$VERIFY_DUPLICATE_RECEIPT" = "true" ]; then
  duplicate_callback_payload="$(jq -cn \
    --arg payment_id "$payment_id" \
    --arg payer_pi_uid "$INITIATOR_PI_UID" \
    --argjson amount_minor_units "$DEPOSIT_MINOR_UNITS" \
    --arg currency_code "$CURRENCY_CODE" \
    --arg txid "$DUPLICATE_TXID" \
    '{payment_id:$payment_id, payer_pi_uid:$payer_pi_uid, amount_minor_units:$amount_minor_units, currency_code:$currency_code, txid:$txid, status:"completed"}')"
  curl -fsS --max-time 5 \
    -H "content-type: application/json" \
    --data "$duplicate_callback_payload" \
    "${BASE_URL}/api/payment/callback" > "$body_file"
  jq -e '
    (.raw_callback_id | type == "string" and length > 0)
    and .duplicate_callback == false
    and (.outbox_event_ids | type == "array" and length == 1)
  ' "$body_file" >/dev/null

  curl -fsS --max-time 10 \
    -H "content-type: application/json" \
    --data '{}' \
    "${BASE_URL}/api/internal/orchestration/drain" > "$body_file"
  jq -e --arg payment_id "$payment_id" '
    .processed_messages
    | any(
        .event_type == "INGEST_PROVIDER_CALLBACK"
        and .provider_submission_id == $payment_id
        and .already_processed == true
      )
  ' "$body_file" >/dev/null

  curl -fsS --max-time 5 \
    -H "authorization: Bearer ${initiator_token}" \
    "${BASE_URL}/api/projection/settlement-views/${settlement_case_id}" > "$body_file"
  jq -e \
    --argjson deposit_amount_minor_units "$DEPOSIT_MINOR_UNITS" \
    --arg currency_code "$CURRENCY_CODE" \
    --arg funded_journal_entry_id "$funded_journal_entry_id" '
    .current_settlement_status == "funded"
    and .total_funded_minor_units == $deposit_amount_minor_units
    and .currency_code == $currency_code
    and .latest_journal_entry_id == $funded_journal_entry_id
  ' "$body_file" >/dev/null

  echo "HTTP funded duplicate-receipt smoke passed at ${BASE_URL}"
  exit 0
fi

echo "HTTP funded happy-route smoke passed at ${BASE_URL}"
