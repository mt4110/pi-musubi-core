# musubi_backend

Day 1 backend app and Rust workspace root.

Current local HTTP surface:
- `POST /api/auth/pi` with explicit `access_token`
- `POST /api/promise/intents`
- `POST /api/payment/callback`
- `POST /api/proof/challenges`
- `POST /api/proof/submissions`
- `POST /api/internal/orchestration/drain` in debug builds, or in release only when `MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_DRAIN=true`
- `GET /api/projection/settlement-views/{settlement_case_id}` for authenticated participants only

## Local infra

The backend now ships with a minimal local development stack:
- PostgreSQL for migrations, orchestration runtime work, and contract tests
- Redis for future cache / queue / coordination work

Important:
the current Issue #21 happy-route path uses PostgreSQL as writer truth for sign-in, Promise / settlement authoring, provider submission mapping, raw callback evidence, receipt idempotency, ledger append, outbox / command-inbox coordination, and the existing settlement-view projection parity path.
See `docs/happy_route_walkthrough.md` for the exact current boundary.

### Start local infra

```bash
cd apps/backend
cp .env.example .env
docker-compose up -d postgres redis
```

### Bootstrap and migrate the writer DB

```bash
cd apps/backend
make db-bootstrap
make db-migrate
make db-status
```

Equivalent raw commands:

```bash
cargo run -p musubi-ops -- db bootstrap
cargo run -p musubi-ops -- db migrate
cargo run -p musubi-ops -- db status
```

Migration tracking lives in `public.musubi_schema_migrations`.
The runner records file checksums, uses a PostgreSQL advisory lock, and treats duplicate migrate runs as no-ops.

### Run the backend on the host

```bash
cd apps/backend
make dev
```

`DATABASE_URL` points at the development database on `127.0.0.1:55432`.
`MUSUBI_TEST_DATABASE_URL` points at the test database on `127.0.0.1:55432`.
`REDIS_URL` points at the local Redis instance on `127.0.0.1:56379`.
`REQUIRE_LATEST_SCHEMA=true` makes backend startup fail if migration tracking is missing, the DB has an applied migration missing from the local checkout, a migration failed, checksum drift exists, or pending migrations remain.

The Day 1 Pi provider adapter is sandbox-only:

- `PROVIDER_MODE=sandbox`
- `PROVIDER_BASE_URL=https://sandbox.minepi.com/v2`
- `PROVIDER_API_KEY`
- `PROVIDER_WEBHOOK_SECRET`
- `PROVIDER_TIMEOUT_MS=3000`

The adapter records provider idempotency mappings and raw callback dedupe in PostgreSQL.
It reads only `PROVIDER_*` settings, so sandbox and production Pi credentials do not silently mix through legacy `PI_API_*` fallback.
`POST /api/payment/callback` is intentionally thin: it saves exact raw body bytes plus redacted headers, records callback dedupe, enqueues `INGEST_PROVIDER_CALLBACK`, and returns without mutating settlement final state.
Normalization, receipt verification, funding, ledger append, and projection refresh run from the orchestration drain / worker side.
If a valid callback arrives before provider submission mapping is visible, callback processing is retried/deferred before any manual-review parking.
Callback signature verification is intentionally skipped for Issue #9 until a pinned Pi callback signature / auth contract exists; raw callback records keep `signature_valid = None` as the future slot.
Those records are durable uniqueness boundaries; they are not a production Pi payment integration yet.

### Run the orchestration contract tests

```bash
cd apps/backend
set -a
. ./.env
set +a
cargo test -p musubi_orchestration
```

### Run the backend checks

```bash
cd apps/backend
cargo check
cargo test
```

## Database skeleton

Issue #3 adds plain SQL migration scaffolding under `migrations/`.
These files establish the Day 1 `core`, `dao`, `ledger`, `outbox`, and `projection` boundaries.

See `docs/schema_skeleton.md` for ownership notes and deferred scope.
Issue #8 adds the runtime migration runner and backend startup schema check.
See `docs/db_runtime.md` for the current DB bootstrap and local reset flow.
Issue #21 wires the happy-route writer truth to PostgreSQL while preserving the existing HTTP surface and settlement-view response contract.
Issue #17 adds the first sandbox Pi provider adapter boundary for happy-route hold submission and callback intake.
ISSUE-10 adds Day 1 safer venue proof primitives.
The public HTTP surface supports the normal venue-code path only.
Proof challenges are short-lived, proof envelopes are server-verified before acceptance, exact replays are rejected with server-keyed replay keys, and venue verification is realm-scoped, server-secret-backed, and bound to the challenge-issued key version.
Operator PIN fallback remains an internal/deferred primitive until an authenticated operator surface exists; subject-facing requests for `operator_pin` are rejected before any operator audit or rate-limit state is touched.
Location hints are sanitized before proof records are built, unsupported fallback modes are rejected before display-code verification, and malformed or cross-subject attempts do not burn another challenge's failed-attempt budget.
These proof records are input facts only; they are not settlement truth or final anti-spoof guarantees.

## Local design notes

- `docs/package_boundaries.md`: crate and ownership boundaries
- `docs/db_runtime.md`: local DB bootstrap, migration runner, and startup schema check
- `docs/schema_skeleton.md`: physical truth boundaries
- `docs/settlement_domain_types.md`: settlement-domain contract
- `docs/orchestration_runtime.md`: outbox/inbox runtime rules
- `docs/guardrails.md`: executable architectural guardrails
- `docs/proof_primitives.md`: Day 1 safer venue proof input boundary
- `docs/happy_route_walkthrough.md`: current Issue #7 end-to-end path
