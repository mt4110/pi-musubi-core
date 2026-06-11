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
PROVIDER_CALLSITE_PATTERN='(?:\.[[:space:]]*|(?:(?:::)?(?:[A-Za-z_][A-Za-z0-9_]*::)*[A-Za-z_][A-Za-z0-9_]*(?:::[[:space:]]*<[^;{}]*>)?|<[^;{}]*[[:space:]]+as[[:space:]]+(?:::)?(?:[A-Za-z_][A-Za-z0-9_]*::)*[A-Za-z_][A-Za-z0-9_]*>)[[:space:]]*::[[:space:]]*)(submit_action|verify_receipt|reconcile_submission|normalize_callback)[[:space:]]*\('
PROVIDER_CALLSITE_INVENTORY_PATH='apps/backend/docs/provider_callsite_inventory.txt'
PUBLIC_ROUTE_RAW_STRING_PATTERN='\br#*"(?:/health(?:"|#)|/api/(?!internal(?:/|"))[^"]*)'
PUBLIC_ROUTE_INVENTORY_PATH='apps/backend/docs/public_route_inventory.txt'
INTERNAL_ROUTE_RAW_STRING_PATTERN='\br#*"/api/internal(?:/|")'
INTERNAL_ROUTE_PREFIX_PATTERN='"/api/internal"'
ROUTE_SPLIT_LITERAL_PATTERN='(?<![A-Za-z0-9_#])"(/api/?|/internal/[^"]*)"'
ROUTE_SPLIT_RAW_STRING_PATTERN='\br#*"(?:/api/?(?:"|#)|/internal/)'
INTERNAL_ROUTE_INVENTORY_PATH='apps/backend/docs/internal_route_inventory.txt'
HTTP_ROUTE_GATE_INVENTORY_PATH='apps/backend/docs/http_route_gate_inventory.txt'
ROUTE_BODY_LIMIT_INVENTORY_PATH='apps/backend/docs/route_body_limit_inventory.txt'
ROUTE_BODY_LIMIT_GAP_INVENTORY_PATH='apps/backend/docs/route_body_limit_gap_inventory.txt'
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

provider_callsite_inventory() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      local match_count
      match_count="$(
        PROVIDER_CALLSITE_PATTERN="$PROVIDER_CALLSITE_PATTERN" perl -0ne '
          BEGIN { $pattern = qr/$ENV{PROVIDER_CALLSITE_PATTERN}/; }
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

transaction_provider_adapter_colocation_matches() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      local raw_transaction_count
      raw_transaction_count="$(
        RAW_TRANSACTION_PATTERN="$RAW_TRANSACTION_PATTERN" perl -0ne '
          BEGIN { $pattern = qr/$ENV{RAW_TRANSACTION_PATTERN}/; }
          while (/$pattern/g) { $count++; }
          END { print $count || 0; }
        ' "$path"
      )"

      local provider_adapter_count
      provider_adapter_count="$(
        PROVIDER_ADAPTER_PATTERN="$PROVIDER_ADAPTER_PATTERN" perl -0ne '
          BEGIN { $pattern = qr/$ENV{PROVIDER_ADAPTER_PATTERN}/; }
          while (/$pattern/g) { $count++; }
          END { print $count || 0; }
        ' "$path"
      )"

      if [ "$raw_transaction_count" -gt 0 ] && [ "$provider_adapter_count" -gt 0 ]; then
        printf "%s:1: raw transaction surface (%s) and provider adapter surface (%s) are co-located\n" \
          "$path" \
          "$raw_transaction_count" \
          "$provider_adapter_count"
      fi
    done
}

transaction_provider_colocation_matches() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      local raw_transaction_count
      raw_transaction_count="$(
        RAW_TRANSACTION_PATTERN="$RAW_TRANSACTION_PATTERN" perl -0ne '
          BEGIN { $pattern = qr/$ENV{RAW_TRANSACTION_PATTERN}/; }
          while (/$pattern/g) { $count++; }
          END { print $count || 0; }
        ' "$path"
      )"

      local provider_callsite_count
      provider_callsite_count="$(
        PROVIDER_CALLSITE_PATTERN="$PROVIDER_CALLSITE_PATTERN" perl -0ne '
          BEGIN { $pattern = qr/$ENV{PROVIDER_CALLSITE_PATTERN}/; }
          while (/$pattern/g) { $count++; }
          END { print $count || 0; }
        ' "$path"
      )"

      if [ "$raw_transaction_count" -gt 0 ] && [ "$provider_callsite_count" -gt 0 ]; then
        printf "%s:1: raw transaction surface (%s) and provider callsite surface (%s) are co-located\n" \
          "$path" \
          "$raw_transaction_count" \
          "$provider_callsite_count"
      fi
    done
}

route_prefix_composition_violations() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      ROUTE_PREFIX_SOURCE_PATH="$path" perl -0ne '
        while (/\.\s*nest(?:_service)?\s*\(\s*"((?:\/api(?:\/[^"]*)?))"/g) {
          my $prefix = $1;
          my $line = 1 + (() = substr($_, 0, $-[0]) =~ /\n/g);
          print "$ENV{ROUTE_PREFIX_SOURCE_PATH}:$line: route prefix $prefix must use full route literals in the route inventories\n";
        }
      ' "$path"
    done
}

public_route_inventory() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      PUBLIC_ROUTE_SOURCE_PATH="$path" perl -0ne '
        while (m{(?<![A-Za-z0-9_#])"(/health(?=")|/api/(?!internal(?:/|"))[^"]*)"}g) {
          my $route = $1;
          my $segment = substr($_, $+[0], 1000);
          if ($segment =~ /(?:^|[\n\r;]|\.)\s*route\s*\(/) {
            $segment = substr($segment, 0, $-[0]);
          }
          my %seen_handlers;
          while ($segment =~ /\b(get|post|put|patch|delete|head|options|trace|any)\s*\(\s*([^,\)\s]+)/g) {
            my $method = uc($1);
            my $handler = $2;
            if ($handler !~ /^[A-Za-z_][A-Za-z0-9_:]*$/ || $handler =~ /^(async|move)$/) {
              $handler = "UNKNOWN_HANDLER";
            }
            $seen_handlers{$method}{$handler} = 1;
            if ($method eq "GET") {
              $seen_handlers{HEAD}{$handler} = 1;
            }
          }
          if (!%seen_handlers) {
            $seen_handlers{UNKNOWN_METHOD}{UNKNOWN_HANDLER} = 1;
          }
          for my $method (sort keys %seen_handlers) {
            for my $handler (sort keys %{ $seen_handlers{$method} }) {
              print "$ENV{PUBLIC_ROUTE_SOURCE_PATH} $route $method $handler\n";
            }
          }
        }
      ' "$path"
    done |
    sort -k1,1 -k2,2 -k3,3 -k4,4
}

internal_route_inventory() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      INTERNAL_ROUTE_SOURCE_PATH="$path" perl -0ne '
        while (m{(?<![A-Za-z0-9_#])"(/api/internal/[^"]*)"}g) {
          my $route = $1;
          my $segment = substr($_, $+[0], 1000);
          if ($segment =~ /(?:^|[\n\r;]|\.)\s*route\s*\(/) {
            $segment = substr($segment, 0, $-[0]);
          }
          my %seen_handlers;
          while ($segment =~ /\b(get|post|put|patch|delete|head|options|trace|any)\s*\(\s*([^,\)\s]+)/g) {
            my $method = uc($1);
            my $handler = $2;
            if ($handler !~ /^[A-Za-z_][A-Za-z0-9_:]*$/ || $handler =~ /^(async|move)$/) {
              $handler = "UNKNOWN_HANDLER";
            }
            $seen_handlers{$method}{$handler} = 1;
            if ($method eq "GET") {
              $seen_handlers{HEAD}{$handler} = 1;
            }
          }
          if (!%seen_handlers) {
            $seen_handlers{UNKNOWN_METHOD}{UNKNOWN_HANDLER} = 1;
          }
          for my $method (sort keys %seen_handlers) {
            for my $handler (sort keys %{ $seen_handlers{$method} }) {
              print "$ENV{INTERNAL_ROUTE_SOURCE_PATH} $route $method $handler\n";
            }
          }
        }
      ' "$path"
    done |
    sort -k1,1 -k2,2 -k3,3 -k4,4
}

http_route_gate_inventory() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      HTTP_ROUTE_GATE_SOURCE_PATH="$path" perl -0ne '
        sub route_gate_for_position {
          my ($source, $position) = @_;
          my $gate = "ALWAYS";

          while ($source =~ /let\s+app\s*=\s*if\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)\s*\{/g) {
            my $candidate_gate = $1;
            my $gate_start = $+[0];
            next if $gate_start > $position;

            my $tail = substr($source, $gate_start);
            if ($tail =~ /\n\s*\}\s*else\s*\{/) {
              my $gate_end = $gate_start + $-[0];
              if ($position >= $gate_start && $position < $gate_end) {
                $gate = $candidate_gate;
              }
            }
          }

          return $gate;
        }

        while (m{(?<![A-Za-z0-9_#])"(/health(?=")|/api/[^"]*)"}g) {
          my $route_start = $-[0];
          my $route = $1;
          my $gate = route_gate_for_position($_, $route_start);
          my $segment = substr($_, $+[0], 1000);
          if ($segment =~ /(?:^|[\n\r;]|\.)\s*route\s*\(/) {
            $segment = substr($segment, 0, $-[0]);
          }
          my %seen_handlers;
          while ($segment =~ /\b(get|post|put|patch|delete|head|options|trace|any)\s*\(\s*([^,\)\s]+)/g) {
            my $method = uc($1);
            my $handler = $2;
            if ($handler !~ /^[A-Za-z_][A-Za-z0-9_:]*$/ || $handler =~ /^(async|move)$/) {
              $handler = "UNKNOWN_HANDLER";
            }
            $seen_handlers{$method}{$handler} = 1;
            if ($method eq "GET") {
              $seen_handlers{HEAD}{$handler} = 1;
            }
          }
          if (!%seen_handlers) {
            $seen_handlers{UNKNOWN_METHOD}{UNKNOWN_HANDLER} = 1;
          }
          for my $method (sort keys %seen_handlers) {
            for my $handler (sort keys %{ $seen_handlers{$method} }) {
              print "$ENV{HTTP_ROUTE_GATE_SOURCE_PATH} $route $method $handler $gate\n";
            }
          }
        }
      ' "$path"
    done |
    sort -k1,1 -k2,2 -k3,3 -k4,4 -k5,5
}

route_body_limit_inventory() {
  git ls-files -- apps/backend/src apps/backend/crates |
    awk -v pattern="$PRODUCTION_SOURCE_PATTERN" '$0 ~ pattern { print }' |
    while IFS= read -r path; do
      ROUTE_BODY_LIMIT_SOURCE_PATH="$path" perl -0ne '
        while (m{(?<![A-Za-z0-9_#])"(/health(?=")|/api/[^"]*)"}g) {
          my $route = $1;
          my $segment = substr($_, $+[0], 1000);
          if ($segment =~ /(?:^|[\n\r;]|\.)\s*route\s*\(/) {
            $segment = substr($segment, 0, $-[0]);
          }

          my $scan_offset = 0;
          while ($segment =~ /\bDefaultBodyLimit::max\s*\(\s*([^)]+?)\s*\)/sg) {
            my $limit_start = $-[0];
            my $limit_end = $+[0];
            my $limit = $1;
            $limit =~ s/\s+//g;

            my $limited_segment = substr($segment, $scan_offset, $limit_start - $scan_offset);
            $scan_offset = $limit_end;

            my %seen_handlers;
            while ($limited_segment =~ /\b(get|post|put|patch|delete|head|options|trace|any)\s*\(\s*([^,\)\s]+)/g) {
              my $method = uc($1);
              my $handler = $2;
              if ($handler !~ /^[A-Za-z_][A-Za-z0-9_:]*$/ || $handler =~ /^(async|move)$/) {
                $handler = "UNKNOWN_HANDLER";
              }
              $seen_handlers{$method}{$handler} = 1;
              if ($method eq "GET") {
                $seen_handlers{HEAD}{$handler} = 1;
              }
            }
            if (!%seen_handlers) {
              $seen_handlers{UNKNOWN_METHOD}{UNKNOWN_HANDLER} = 1;
            }
            for my $method (sort keys %seen_handlers) {
              for my $handler (sort keys %{ $seen_handlers{$method} }) {
                print "$ENV{ROUTE_BODY_LIMIT_SOURCE_PATH} $route $method $handler DefaultBodyLimit::max($limit)\n";
              }
            }
          }
        }
      ' "$path"
    done |
    sort -k1,1 -k2,2 -k3,3 -k4,4 -k5,5
}

state_changing_route_inventory() {
  {
    public_route_inventory
    internal_route_inventory
  } |
    awk '$3 ~ /^(POST|PUT|PATCH|DELETE|ANY)$/ { print }' |
    sort -k1,1 -k2,2 -k3,3 -k4,4
}

route_body_limit_gap_inventory() {
  local all_state_changing_path
  local limited_state_changing_path

  all_state_changing_path="$(mktemp)"
  limited_state_changing_path="$(mktemp)"

  state_changing_route_inventory > "$all_state_changing_path"
  route_body_limit_inventory |
    awk '$3 ~ /^(POST|PUT|PATCH|DELETE|ANY)$/ { print $1 " " $2 " " $3 " " $4 }' |
    sort -k1,1 -k2,2 -k3,3 -k4,4 > "$limited_state_changing_path"

  comm -23 "$all_state_changing_path" "$limited_state_changing_path"

  rm -f "$all_state_changing_path" "$limited_state_changing_path"
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

check_provider_callsite_inventory() {
  local expected_path="$repo_root/$PROVIDER_CALLSITE_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "provider callsite inventory file is missing"
    return
  fi

  provider_callsite_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: provider callsite inventory matches the reviewed baseline"
  else
    report_failure "provider callsite inventory changed; review provider I/O boundary and no-Tx-across-await shape before updating baseline"
  fi

  rm -f "$actual_path"
}

check_transaction_provider_adapter_colocation() {
  local matches
  matches="$(transaction_provider_adapter_colocation_matches)"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "production files must not co-locate raw DB transaction surface with provider adapter surface"
  else
    echo "ok: production transaction and provider adapter surfaces stay separated by file"
  fi
}

check_transaction_provider_colocation() {
  local matches
  matches="$(transaction_provider_colocation_matches)"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "production files must not co-locate raw DB transaction surface with provider callsites"
  else
    echo "ok: production transaction and provider callsite surfaces stay separated by file"
  fi
}

check_route_prefix_composition() {
  local matches
  matches="$(route_prefix_composition_violations)"
  if [ -n "$matches" ]; then
    echo "$matches" >&2
    report_failure "production routes must not use nested /api route prefixes that hide child route surface from inventories"
  else
    echo "ok: production routes avoid nested /api route prefixes"
  fi
}

check_public_route_inventory() {
  local expected_path="$repo_root/$PUBLIC_ROUTE_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "public route inventory file is missing"
    return
  fi

  public_route_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: public route inventory matches the reviewed baseline"
  else
    report_failure "public HTTP route/method/handler surface changed; review launch, consent, body limits, and user-facing exposure before updating baseline"
  fi

  rm -f "$actual_path"
}

check_internal_route_inventory() {
  local expected_path="$repo_root/$INTERNAL_ROUTE_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "internal route inventory file is missing"
    return
  fi

  internal_route_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: internal route inventory matches the reviewed baseline"
  else
    report_failure "internal HTTP route/method/handler surface changed; review gate, auth, and operator-only semantics before updating baseline"
  fi

  rm -f "$actual_path"
}

check_http_route_gate_inventory() {
  local expected_path="$repo_root/$HTTP_ROUTE_GATE_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "HTTP route gate inventory file is missing"
    return
  fi

  http_route_gate_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: HTTP route gate inventory matches the reviewed baseline"
  else
    report_failure "HTTP route gate surface changed; review launch, callback, and internal route exposure before updating baseline"
  fi

  rm -f "$actual_path"
}

check_route_body_limit_inventory() {
  local expected_path="$repo_root/$ROUTE_BODY_LIMIT_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "route body limit inventory file is missing"
    return
  fi

  route_body_limit_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: route body limit inventory matches the reviewed baseline"
  else
    report_failure "explicit route body limit surface changed; review request size exposure before updating baseline"
  fi

  rm -f "$actual_path"
}

check_route_body_limit_gap_inventory() {
  local expected_path="$repo_root/$ROUTE_BODY_LIMIT_GAP_INVENTORY_PATH"
  local actual_path
  actual_path="$(mktemp)"

  if [ ! -f "$expected_path" ]; then
    rm -f "$actual_path"
    report_failure "route body limit gap inventory file is missing"
    return
  fi

  route_body_limit_gap_inventory > "$actual_path"
  if diff -u "$expected_path" "$actual_path"; then
    echo "ok: route body limit gap inventory matches the reviewed baseline"
  else
    report_failure "state-changing route body limit gap surface changed; review request-size boundaries before updating baseline"
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
  check_transaction_provider_adapter_colocation
  check_provider_callsite_inventory
  check_transaction_provider_colocation
  check_no_matches \
    "production public routes must use ordinary string literals, not Rust raw strings" \
    "$PUBLIC_ROUTE_RAW_STRING_PATTERN" \
    apps/backend/src \
    apps/backend/crates
  check_route_prefix_composition
  check_public_route_inventory
  check_no_matches \
    "production internal routes must use ordinary string literals, not Rust raw strings" \
    "$INTERNAL_ROUTE_RAW_STRING_PATTERN" \
    apps/backend/src \
    apps/backend/crates
  check_no_matches \
    "production route declarations must not split /api or /internal route prefixes across parent and child literals" \
    "$ROUTE_SPLIT_LITERAL_PATTERN" \
    apps/backend/src \
    apps/backend/crates
  check_no_matches \
    "production route declarations must not split /api or /internal route prefixes across raw-string parent and child literals" \
    "$ROUTE_SPLIT_RAW_STRING_PATTERN" \
    apps/backend/src \
    apps/backend/crates
  check_no_matches \
    "production internal routes must use full /api/internal/... literals instead of nested prefix composition" \
    "$INTERNAL_ROUTE_PREFIX_PATTERN" \
    apps/backend/src \
    apps/backend/crates
  check_internal_route_inventory
  check_http_route_gate_inventory
  check_route_body_limit_inventory
  check_route_body_limit_gap_inventory
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
    printf 'backend.submit_action(cmd).await?; backend.verify_receipt(receipt).await?;\nbackend\n    .normalize_callback(callback)\n    .await?;\nSettlementBackend::submit_action(&backend, cmd).await?;\n<PiSettlementBackend as SettlementBackend>::normalize_callback(&backend, callback).await?;\nsettlement_domain::SettlementBackend::submit_action(&backend, cmd).await?;\n<PiSettlementBackend as settlement_domain::SettlementBackend>::normalize_callback(&backend, callback).await?;\nProviderBackend::verify_receipt(&backend, receipt).await?;\n<PiSettlementBackend as ProviderBackend>::reconcile_submission(&backend, submission).await?;\n' > apps/backend/src/provider_callsite.rs
    printf 'route("/health", get(health));\nroute("/api/auth/pi", post(auth).layer(DefaultBodyLimit::max(32 * 1024)));\nroute("/api/body-limits", post(create).layer(DefaultBodyLimit::max(8 * 1024)).put(update).layer(DefaultBodyLimit::max(16 * 1024)));\nroute("/api/promise/intents", post(intent));\nroute("/api/review-cases/{review_case_id}/appeals", post(create).get(list).layer(DefaultBodyLimit::max(8 * 1024)));\nlet app = if public_callback_enabled() {\n  app.route("/api/payment/callback", handler)\n} else { app };\n' > apps/backend/src/public_routes.rs
    printf 'route(r#"/api/auth/pi"#, post(auth));\nroute(r#"/health"#, get(health));\n' > apps/backend/src/public_route_raw.rs
    printf 'nest("/api/", Router::new().route("auth/pi", handler));\n' > apps/backend/tests/public_route_split.rs
    printf 'nest(r#"/api/"#, Router::new().route("auth/pi", handler));\n' > apps/backend/tests/public_route_split_raw.rs
    printf 'let app = if internal_gate_enabled() {\n  app.route("/api/internal/orchestration/drain", post(handler).layer(DefaultBodyLimit::max(16 * 1024)))\n    .route("/api/internal/operator/review-cases/{review_case_id}", get(read).post(write))\n    .route("/api/internal/ops/unknown-method", handler)\n} else { app };\n' > apps/backend/src/internal_routes.rs
    printf 'nest("/api/internal", Router::new().route("/ops/foo", handler));\n' > apps/backend/src/internal_route_prefix.rs
    printf 'route(r#"/api/internal/ops/foo"#, handler);\n' > apps/backend/src/internal_route_raw.rs
    printf 'nest("/api", Router::new().route("/internal/ops/foo", handler));\n' > apps/backend/src/internal_route_split.rs
    printf 'nest(r#"/api"#, Router::new().route(r#"/internal/ops/foo"#, handler));\n' > apps/backend/src/internal_route_split_raw.rs
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

    printf 'let tx = client.transaction().await?;\nstruct InlineSettlementBackend;\nimpl SettlementBackend for InlineSettlementBackend {}\n' > apps/backend/src/transaction_provider_adapter_colocation.rs
    git add apps/backend/src/transaction_provider_adapter_colocation.rs

    if [ "$(transaction_provider_adapter_colocation_matches)" = "$(printf 'apps/backend/src/transaction_provider_adapter_colocation.rs:1: raw transaction surface (1) and provider adapter surface (2) are co-located')" ]; then
      echo "ok: self-test catches transaction and provider adapter co-location"
    else
      transaction_provider_adapter_colocation_matches >&2
      report_failure "self-test catches transaction and provider adapter co-location"
    fi

    if [ "$(provider_callsite_inventory)" = "$(printf '9 apps/backend/src/provider_callsite.rs')" ]; then
      echo "ok: self-test builds the provider callsite inventory"
    else
      provider_callsite_inventory >&2
      report_failure "self-test builds the provider callsite inventory"
    fi

    printf 'let tx = client.transaction().await?;\nbackend.submit_action(cmd).await?;\n' > apps/backend/src/transaction_provider_colocation.rs
    git add apps/backend/src/transaction_provider_colocation.rs

    if [ "$(transaction_provider_colocation_matches)" = "$(printf 'apps/backend/src/transaction_provider_colocation.rs:1: raw transaction surface (1) and provider callsite surface (1) are co-located')" ]; then
      echo "ok: self-test catches transaction and provider callsite co-location"
    else
      transaction_provider_colocation_matches >&2
      report_failure "self-test catches transaction and provider callsite co-location"
    fi

    assert_matches \
      "self-test catches raw-string public route literals" \
      "$PUBLIC_ROUTE_RAW_STRING_PATTERN" \
      apps/backend/src/public_route_raw.rs

    if [ "$(public_route_inventory)" = "$(printf 'apps/backend/src/public_routes.rs /api/auth/pi POST auth\napps/backend/src/public_routes.rs /api/body-limits POST create\napps/backend/src/public_routes.rs /api/body-limits PUT update\napps/backend/src/public_routes.rs /api/payment/callback UNKNOWN_METHOD UNKNOWN_HANDLER\napps/backend/src/public_routes.rs /api/promise/intents POST intent\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals GET list\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals HEAD list\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals POST create\napps/backend/src/public_routes.rs /health GET health\napps/backend/src/public_routes.rs /health HEAD health')" ]; then
      echo "ok: self-test builds the public route inventory"
    else
      public_route_inventory >&2
      report_failure "self-test builds the public route inventory"
    fi

    printf 'Router::new()\n  .nest(\n    "/api/auth",\n    Router::new().route("/pi", post(handler)),\n  )\n  .nest_service("/api/proxy", svc);\n' > apps/backend/src/public_route_nested_prefix.rs
    git add apps/backend/src/public_route_nested_prefix.rs

    if [ "$(route_prefix_composition_violations)" = "$(printf 'apps/backend/src/public_route_nested_prefix.rs:2: route prefix /api/auth must use full route literals in the route inventories\napps/backend/src/public_route_nested_prefix.rs:6: route prefix /api/proxy must use full route literals in the route inventories')" ]; then
      echo "ok: self-test catches nested public route prefixes"
    else
      route_prefix_composition_violations >&2
      report_failure "self-test catches nested public route prefixes"
    fi

    assert_matches \
      "self-test catches split public route literals" \
      "$ROUTE_SPLIT_LITERAL_PATTERN" \
      apps/backend/tests/public_route_split.rs

    assert_matches \
      "self-test catches split public raw-string route literals" \
      "$ROUTE_SPLIT_RAW_STRING_PATTERN" \
      apps/backend/tests/public_route_split_raw.rs

    assert_matches \
      "self-test catches nested internal route prefixes" \
      "$INTERNAL_ROUTE_PREFIX_PATTERN" \
      apps/backend/src/internal_route_prefix.rs

    assert_matches \
      "self-test catches raw-string internal route literals" \
      "$INTERNAL_ROUTE_RAW_STRING_PATTERN" \
      apps/backend/src/internal_route_raw.rs

    assert_matches \
      "self-test catches split internal route literals" \
      "$ROUTE_SPLIT_LITERAL_PATTERN" \
      apps/backend/src/internal_route_split.rs

    assert_matches \
      "self-test catches split raw-string internal route literals" \
      "$ROUTE_SPLIT_RAW_STRING_PATTERN" \
      apps/backend/src/internal_route_split_raw.rs

    if [ "$(internal_route_inventory)" = "$(printf 'apps/backend/src/internal_routes.rs /api/internal/operator/review-cases/{review_case_id} GET read\napps/backend/src/internal_routes.rs /api/internal/operator/review-cases/{review_case_id} HEAD read\napps/backend/src/internal_routes.rs /api/internal/operator/review-cases/{review_case_id} POST write\napps/backend/src/internal_routes.rs /api/internal/ops/unknown-method UNKNOWN_METHOD UNKNOWN_HANDLER\napps/backend/src/internal_routes.rs /api/internal/orchestration/drain POST handler')" ]; then
      echo "ok: self-test builds the internal route inventory"
    else
      internal_route_inventory >&2
      report_failure "self-test builds the internal route inventory"
    fi

    if [ "$(route_body_limit_inventory)" = "$(printf 'apps/backend/src/internal_routes.rs /api/internal/orchestration/drain POST handler DefaultBodyLimit::max(16*1024)\napps/backend/src/public_routes.rs /api/auth/pi POST auth DefaultBodyLimit::max(32*1024)\napps/backend/src/public_routes.rs /api/body-limits POST create DefaultBodyLimit::max(8*1024)\napps/backend/src/public_routes.rs /api/body-limits PUT update DefaultBodyLimit::max(16*1024)\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals GET list DefaultBodyLimit::max(8*1024)\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals HEAD list DefaultBodyLimit::max(8*1024)\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals POST create DefaultBodyLimit::max(8*1024)')" ]; then
      echo "ok: self-test builds the route body limit inventory"
    else
      route_body_limit_inventory >&2
      report_failure "self-test builds the route body limit inventory"
    fi

    if [ "$(route_body_limit_gap_inventory)" = "$(printf 'apps/backend/src/internal_routes.rs /api/internal/operator/review-cases/{review_case_id} POST write\napps/backend/src/public_routes.rs /api/promise/intents POST intent')" ]; then
      echo "ok: self-test builds the route body limit gap inventory"
    else
      route_body_limit_gap_inventory >&2
      report_failure "self-test builds the route body limit gap inventory"
    fi

    if [ "$(http_route_gate_inventory)" = "$(printf 'apps/backend/src/internal_route_prefix.rs /api/internal UNKNOWN_METHOD UNKNOWN_HANDLER ALWAYS\napps/backend/src/internal_routes.rs /api/internal/operator/review-cases/{review_case_id} GET read internal_gate_enabled\napps/backend/src/internal_routes.rs /api/internal/operator/review-cases/{review_case_id} HEAD read internal_gate_enabled\napps/backend/src/internal_routes.rs /api/internal/operator/review-cases/{review_case_id} POST write internal_gate_enabled\napps/backend/src/internal_routes.rs /api/internal/ops/unknown-method UNKNOWN_METHOD UNKNOWN_HANDLER internal_gate_enabled\napps/backend/src/internal_routes.rs /api/internal/orchestration/drain POST handler internal_gate_enabled\napps/backend/src/public_route_nested_prefix.rs /api/auth UNKNOWN_METHOD UNKNOWN_HANDLER ALWAYS\napps/backend/src/public_route_nested_prefix.rs /api/proxy UNKNOWN_METHOD UNKNOWN_HANDLER ALWAYS\napps/backend/src/public_routes.rs /api/auth/pi POST auth ALWAYS\napps/backend/src/public_routes.rs /api/body-limits POST create ALWAYS\napps/backend/src/public_routes.rs /api/body-limits PUT update ALWAYS\napps/backend/src/public_routes.rs /api/payment/callback UNKNOWN_METHOD UNKNOWN_HANDLER public_callback_enabled\napps/backend/src/public_routes.rs /api/promise/intents POST intent ALWAYS\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals GET list ALWAYS\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals HEAD list ALWAYS\napps/backend/src/public_routes.rs /api/review-cases/{review_case_id}/appeals POST create ALWAYS\napps/backend/src/public_routes.rs /health GET health ALWAYS\napps/backend/src/public_routes.rs /health HEAD health ALWAYS')" ]; then
      echo "ok: self-test builds the HTTP route gate inventory"
    else
      http_route_gate_inventory >&2
      report_failure "self-test builds the HTTP route gate inventory"
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
