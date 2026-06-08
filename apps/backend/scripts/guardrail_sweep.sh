#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"

failures=0

FLOAT_MONEY_PATTERN='\b(f32|f64)\b|double[[:space:]]+precision|\breal\b|\bfloat[0-9]*\b'
NETWORK_CLIENT_PATTERN='\b(reqwest|ureq|surf|hyper::Client|awc::Client|TcpStream|tokio::net::TcpStream|isahc)\b|curl::'
RAW_TRANSACTION_PATTERN="\\.transaction\\(|\\b(?:tokio_postgres::)?Transaction<'[A-Za-z_][A-Za-z0-9_]*>"
RAW_TRANSACTION_INVENTORY_PATH='apps/backend/docs/raw_transaction_inventory.txt'
COORDINATION_PRUNE_PATTERN='(?i)delete[[:space:]]+from[[:space:]]+outbox\.(events|command_inbox)\b'
COORDINATION_PRUNE_INVENTORY_PATH='apps/backend/docs/coordination_prune_inventory.txt'
PRODUCTION_SOURCE_PATTERN='^apps/backend/(src/|crates/[^/]+/src/)'
PROVIDER_ADAPTER_PATTERN='\btrait[[:space:]]+SettlementBackend\b|\bimpl[[:space:]]+SettlementBackend[[:space:]]+for\b|\bstruct[[:space:]]+[A-Za-z0-9_]*(SettlementBackend|ProviderClient|ProviderConfig)\b|\b(derive_provider_idempotency_key|verify_receipt_impl|submit_action_impl|reconcile_submission_impl|normalize_callback_impl)\b'
PROVIDER_ADAPTER_INVENTORY_PATH='apps/backend/docs/provider_adapter_inventory.txt'
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

coordination_prune_inventory() {
  git ls-files -- \
    apps/backend/src \
    apps/backend/crates \
    apps/backend/tests \
    apps/backend/migrations |
    while IFS= read -r path; do
      local match_count
      match_count="$(
        COORDINATION_PRUNE_PATTERN="$COORDINATION_PRUNE_PATTERN" perl -0ne '
          BEGIN { $pattern = qr/$ENV{COORDINATION_PRUNE_PATTERN}/; }
          while (/$pattern/g) { $count++; }
          END { print $count || 0; }
        ' "$path"
      )"
      if [ "$match_count" -gt 0 ]; then
        printf "%d %s\n" "$match_count" "$path"
      fi
    done |
    sort -k2,2
}

archive_before_prune_violations() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      ARCHIVE_BEFORE_PRUNE_PATH="$path" perl -0ne '
        my $path = $ENV{ARCHIVE_BEFORE_PRUNE_PATH};
        while (/(?i)delete[[:space:]]+from[[:space:]]+outbox\.(events|command_inbox)\b/g) {
          my $table = lc($1);
          my $delete_start = $-[0];
          my $line = 1 + (() = substr($_, 0, $delete_start) =~ /\n/g);

          my $prefix = substr($_, 0, $delete_start);
          my $last_same_delete_end = 0;
          my $same_delete_pattern = qr/(?i)delete[[:space:]]+from[[:space:]]+outbox\.\Q$table\E\b/;
          while ($prefix =~ /$same_delete_pattern/g) {
            $last_same_delete_end = $+[0];
          }

          my $segment = substr($prefix, $last_same_delete_end);
          my @required_archives =
            $table eq "events"
              ? ("outbox.outbox_event_archive", "outbox.outbox_attempt_archive")
              : ("outbox.command_inbox_archive");

          for my $archive (@required_archives) {
            my $archive_pattern = qr/(?i)insert[[:space:]]+into[[:space:]]+\Q$archive\E\b/;
            if ($segment !~ /$archive_pattern/) {
              print "$path:$line: DELETE FROM outbox.$table must be preceded by INSERT INTO $archive before pruning\n";
            }
          }
        }
      ' "$path"
    done
}

provider_adapter_inventory() {
  local source_paths=(apps/backend/src apps/backend/crates/*/src)

  grep_matches \
    "$PROVIDER_ADAPTER_PATTERN" \
    "${source_paths[@]}" |
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

check_coordination_prune_inventory() {
  local expected_path="$repo_root/$COORDINATION_PRUNE_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "coordination prune inventory file is missing"
    return
  fi

  coordination_prune_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: coordination prune inventory matches the reviewed baseline"
  else
    report_failure "coordination hot-table prune/delete surface changed; review archive-before-prune behavior before updating baseline"
  fi

  rm -f "$actual_path"
}

check_archive_before_prune() {
  local matches
  matches="$(archive_before_prune_violations)"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "production coordination hot-table pruning must archive before deleting"
  else
    echo "ok: production coordination hot-table pruning archives before deleting"
  fi
}

check_provider_adapter_inventory() {
  local expected_path="$repo_root/$PROVIDER_ADAPTER_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "provider adapter inventory file is missing"
    return
  fi

  provider_adapter_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: provider adapter inventory matches the reviewed baseline"
  else
    report_failure "provider adapter inventory changed; review settlement/provider boundary surface before updating baseline"
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
  check_coordination_prune_inventory
  check_archive_before_prune
  check_provider_adapter_inventory
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
      apps/backend/crates/settlement-domain/src \
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
    printf 'INSERT INTO outbox.outbox_event_archive SELECT event_id FROM outbox.events;\nINSERT INTO outbox.outbox_attempt_archive SELECT event_id FROM outbox.outbox_attempts;\ndelete FROM outbox.events WHERE retain_until < CURRENT_TIMESTAMP;\nINSERT INTO outbox.command_inbox_archive SELECT command_id FROM outbox.command_inbox;\nDeLeTe\nfrom outbox.command_inbox WHERE retain_until < CURRENT_TIMESTAMP;\n' > apps/backend/src/coordination_prune.rs
    printf 'delete FROM outbox.events WHERE retain_until < CURRENT_TIMESTAMP;\nDeLeTe\nfrom outbox.command_inbox WHERE retain_until < CURRENT_TIMESTAMP;\n' > apps/backend/src/coordination_prune_without_archive.rs
    printf 'pub trait SettlementBackend { async fn submit_action_impl(&self); }\n' > apps/backend/crates/settlement-domain/src/backend.rs
    printf 'struct SandboxPiProviderClient;\nimpl SettlementBackend for PiSettlementBackend {}\n' > apps/backend/src/provider_adapter.rs
    printf "tx: &tokio_postgres::Transaction<'tx>,\n" > apps/backend/tests/raw_transaction_ref.rs
    printf "tx: &Transaction<'txn>,\n" > apps/backend/tests/raw_transaction_imported_ref.rs
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

    if [ "$(raw_transaction_inventory)" = "$(printf '1 apps/backend/src/raw_transaction.rs\n1 apps/backend/tests/raw_transaction_imported_ref.rs\n1 apps/backend/tests/raw_transaction_ref.rs')" ]; then
      echo "ok: self-test builds the raw transaction inventory"
    else
      raw_transaction_inventory >&2
      report_failure "self-test builds the raw transaction inventory"
    fi

    if [ "$(coordination_prune_inventory)" = "$(printf '2 apps/backend/src/coordination_prune.rs\n2 apps/backend/src/coordination_prune_without_archive.rs')" ]; then
      echo "ok: self-test builds the coordination prune inventory"
    else
      coordination_prune_inventory >&2
      report_failure "self-test builds the coordination prune inventory"
    fi

    if [ "$(archive_before_prune_violations)" = "$(printf 'apps/backend/src/coordination_prune_without_archive.rs:1: DELETE FROM outbox.events must be preceded by INSERT INTO outbox.outbox_event_archive before pruning\napps/backend/src/coordination_prune_without_archive.rs:1: DELETE FROM outbox.events must be preceded by INSERT INTO outbox.outbox_attempt_archive before pruning\napps/backend/src/coordination_prune_without_archive.rs:2: DELETE FROM outbox.command_inbox must be preceded by INSERT INTO outbox.command_inbox_archive before pruning')" ]; then
      echo "ok: self-test catches production pruning without archives"
    else
      archive_before_prune_violations >&2
      report_failure "self-test catches production pruning without archives"
    fi

    if [ "$(provider_adapter_inventory)" = "$(printf '1 apps/backend/crates/settlement-domain/src/backend.rs\n2 apps/backend/src/provider_adapter.rs')" ]; then
      echo "ok: self-test builds the provider adapter inventory"
    else
      provider_adapter_inventory >&2
      report_failure "self-test builds the provider adapter inventory"
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
