#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "$repo_root"

failures=0

report_failure() {
  echo "::error::$1" >&2
  failures=1
}

check_no_matches() {
  local description="$1"
  local pattern="$2"
  shift 2

  local matches
  matches="$(git grep -n --perl-regexp "$pattern" -- "$@" || true)"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "$description"
  else
    echo "ok: $description"
  fi
}

check_no_matches \
  "backend source and migrations must not introduce floating-point money primitives" \
  '\b(f32|f64)\b|double[[:space:]]+precision|\breal\b|\bfloat[0-9]*\b' \
  apps/backend/src \
  apps/backend/crates \
  apps/backend/migrations

check_no_matches \
  "backend source must not introduce direct external network clients outside an explicit reviewed boundary" \
  '\b(reqwest|ureq|surf|hyper::Client|awc::Client|TcpStream|tokio::net::TcpStream|isahc)\b|curl::' \
  apps/backend/Cargo.toml \
  apps/backend/crates/*/Cargo.toml \
  apps/backend/src \
  apps/backend/crates

read_replica_matches="$(
  git grep -n 'WriterReadSource::ReadReplica' -- \
    apps/backend/src \
    apps/backend/crates \
    ':!apps/backend/crates/orchestration/src/store.rs' \
    ':!apps/backend/crates/orchestration/tests/runtime_contract.rs' \
    || true
)"
if [ -n "$read_replica_matches" ]; then
  echo "$read_replica_matches" >&2
  report_failure "ReadReplica must stay confined to the orchestration rejection implementation and tests"
else
  echo "ok: ReadReplica stays confined to rejection implementation and tests"
fi

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

if [ "$failures" -ne 0 ]; then
  exit 1
fi

echo "backend guardrail sweep passed"
