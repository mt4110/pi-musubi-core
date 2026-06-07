#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"

failures=0

FLOAT_MONEY_PATTERN='\b(f32|f64)\b|double[[:space:]]+precision|\breal\b|\bfloat[0-9]*\b'
NETWORK_CLIENT_PATTERN='\b(reqwest|ureq|surf|hyper::Client|awc::Client|TcpStream|tokio::net::TcpStream|isahc)\b|curl::'
RAW_TRANSACTION_PATTERN="\\.transaction\\(|\\bTransaction<'_>|tokio_postgres::Transaction<'_>"
RAW_TRANSACTION_INVENTORY_PATH='apps/backend/docs/raw_transaction_inventory.txt'
READ_REPLICA_TOKEN='WriterReadSource::ReadReplica'

report_failure() {
  echo "::error::$1" >&2
  failures=1
}

grep_matches() {
  local pattern="$1"
  shift

  git grep -n --perl-regexp "$pattern" -- "$@" || true
}

check_no_matches() {
  local description="$1"
  local pattern="$2"
  shift 2

  local matches
  matches="$(grep_matches "$pattern" "$@")"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "$description"
  else
    echo "ok: $description"
  fi
}

assert_matches() {
  local description="$1"
  local pattern="$2"
  shift 2

  local matches
  matches="$(grep_matches "$pattern" "$@")"
  if [ -n "$matches" ]; then
    echo "ok: $description"
  else
    report_failure "$description"
  fi
}

assert_no_matches() {
  local description="$1"
  local pattern="$2"
  shift 2

  local matches
  matches="$(grep_matches "$pattern" "$@")"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "$description"
  else
    echo "ok: $description"
  fi
}

check_read_replica_boundary() {
  local matches
  matches="$(read_replica_boundary_matches)"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "ReadReplica must stay confined to the orchestration rejection implementation and tests"
  else
    echo "ok: ReadReplica stays confined to rejection implementation and tests"
  fi
}

raw_transaction_inventory() {
  grep_matches \
    "$RAW_TRANSACTION_PATTERN" \
    apps/backend/src \
    apps/backend/crates \
    apps/backend/tests |
    awk -F: '{ count[$1]++ } END { for (path in count) printf "%d %s\n", count[path], path }' |
    sort -k2,2
}

check_raw_transaction_inventory() {
  local expected_path="$repo_root/$RAW_TRANSACTION_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "raw transaction inventory file is missing"
    return
  fi

  raw_transaction_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: raw transaction inventory matches the reviewed baseline"
  else
    report_failure "raw transaction inventory changed; review DB transaction surface before updating baseline"
  fi

  rm -f "$actual_path"
}

read_replica_boundary_matches() {
  git grep -n "$READ_REPLICA_TOKEN" -- \
    apps/backend/src \
    apps/backend/crates \
    ':!apps/backend/crates/orchestration/src/store.rs' \
    ':!apps/backend/crates/orchestration/tests/runtime_contract.rs' \
    || true
}

check_codex_hygiene() {
  local tracked_codex
  tracked_codex="$(git ls-files .codex)"
  if [ -n "$tracked_codex" ]; then
    echo "$tracked_codex" >&2
    report_failure ".codex is local agent state and must not be tracked"
  else
    echo "ok: .codex is not tracked"
  fi

  if git grep -q -E '^\.codex/$' -- .gitignore; then
    echo "ok: .codex remains ignored"
  else
    report_failure ".gitignore must keep .codex/ ignored"
  fi
}

run_sweep() {
  cd "$repo_root"

  check_no_matches \
    "backend source and migrations must not introduce floating-point money primitives" \
    "$FLOAT_MONEY_PATTERN" \
    apps/backend/src \
    apps/backend/crates \
    apps/backend/migrations

  check_no_matches \
    "backend source must not introduce direct external network clients outside an explicit reviewed boundary" \
    "$NETWORK_CLIENT_PATTERN" \
    apps/backend/Cargo.toml \
    apps/backend/crates/*/Cargo.toml \
    apps/backend/src \
    apps/backend/crates

  check_raw_transaction_inventory
  check_read_replica_boundary
  check_codex_hygiene
}

run_self_test() {
  local temp_dir
  temp_dir="$(mktemp -d)"

  if ! (
    cleanup_self_test() {
      rm -rf "$temp_dir"
    }
    trap cleanup_self_test EXIT

    cd "$temp_dir"
    git init -q

    mkdir -p \
      .codex \
      apps/backend/src \
      apps/backend/crates/orchestration/src \
      apps/backend/crates/orchestration/tests \
      apps/backend/tests \
      apps/backend/migrations

    printf '.codex/\n' > .gitignore
    printf 'let amount: f64 = 1.0;\n' > apps/backend/src/float_money.rs
    printf 'let label = "really reality token";\n' > apps/backend/src/word_boundary_safe.rs
    printf 'let client = reqwest::Client::new();\n' > apps/backend/src/reqwest_client.rs
    printf 'let client = curl::easy::Easy::new();\n' > apps/backend/src/curl_client.rs
    printf 'let tx = client.transaction().await?;\n' > apps/backend/src/raw_transaction.rs
    printf "tx: &tokio_postgres::Transaction<'_>,\n" > apps/backend/tests/raw_transaction_ref.rs
    printf 'let source = WriterReadSource::ReadReplica;\n' > apps/backend/crates/orchestration/src/replica_leak.rs
    printf 'let source = WriterReadSource::ReadReplica;\n' > apps/backend/crates/orchestration/src/store.rs
    printf 'let source = WriterReadSource::ReadReplica;\n' > apps/backend/crates/orchestration/tests/runtime_contract.rs
    printf 'local state\n' > .codex/state

    git add .gitignore apps
    git add -f .codex/state

    assert_matches \
      "self-test catches floating-point money primitives" \
      "$FLOAT_MONEY_PATTERN" \
      apps/backend/src/float_money.rs

    assert_no_matches \
      "self-test keeps word boundaries from overmatching ordinary words" \
      "$FLOAT_MONEY_PATTERN" \
      apps/backend/src/word_boundary_safe.rs

    assert_matches \
      "self-test catches word-boundary network clients" \
      "$NETWORK_CLIENT_PATTERN" \
      apps/backend/src/reqwest_client.rs

    assert_matches \
      "self-test catches curl namespace clients" \
      "$NETWORK_CLIENT_PATTERN" \
      apps/backend/src/curl_client.rs

    if [ "$(raw_transaction_inventory)" = "$(printf '1 apps/backend/src/raw_transaction.rs\n1 apps/backend/tests/raw_transaction_ref.rs')" ]; then
      echo "ok: self-test builds the raw transaction inventory"
    else
      raw_transaction_inventory >&2
      report_failure "self-test builds the raw transaction inventory"
    fi

    if [ -n "$(read_replica_boundary_matches)" ]; then
      echo "ok: self-test catches ReadReplica outside the allowlist"
    else
      report_failure "self-test catches ReadReplica outside the allowlist"
    fi

    if [ -n "$(git ls-files .codex)" ]; then
      echo "ok: self-test catches tracked .codex files"
    else
      report_failure "self-test catches tracked .codex files"
    fi

    if git grep -q -E '^\.codex/$' -- .gitignore; then
      echo "ok: self-test verifies .codex/ ignore detection"
    else
      report_failure "self-test verifies .codex/ ignore detection"
    fi

    if [ "$failures" -ne 0 ]; then
      exit 1
    fi
  ); then
    failures=1
  fi
}

case "${1:-}" in
  "")
    run_sweep
    ;;
  --self-test)
    run_self_test
    ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac

if [ "$failures" -ne 0 ]; then
  exit 1
fi

if [ "${1:-}" = "--self-test" ]; then
  echo "backend guardrail sweep self-test passed"
else
  echo "backend guardrail sweep passed"
fi
