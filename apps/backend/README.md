# musubi_backend

Day 1 backend app and Rust workspace root.

Current local HTTP surface:
- `POST /api/auth/pi` with explicit `access_token`
- `POST /api/promise/intents`
- `POST /api/payment/callback`
- `POST /api/internal/orchestration/drain` in debug builds, or in release only when `MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_DRAIN=true`
- `GET /api/projection/settlement-views/{settlement_case_id}` for authenticated participants only

## Local infra

The backend now ships with a minimal local development stack:
- PostgreSQL for migrations, orchestration runtime work, and contract tests
- Redis for future cache / queue / coordination work

Important:
the current Issue #7 happy-route demo still keeps its authoritative state in an in-memory stand-in.
PostgreSQL is the target truth boundary for the long-term implementation, but the local Axum happy route has not been fully rewired to it yet.
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
These files establish the Day 1 `core`, `dao`, `ledger`, `outbox`, and `projection` boundaries without adding runtime DB wiring yet.

See `docs/schema_skeleton.md` for ownership notes and deferred scope.
Issue #8 adds the runtime migration runner and backend startup schema check.
See `docs/db_runtime.md` for the current DB bootstrap and local reset flow.

## Local design notes

- `docs/package_boundaries.md`: crate and ownership boundaries
- `docs/db_runtime.md`: local DB bootstrap, migration runner, and startup schema check
- `docs/schema_skeleton.md`: physical truth boundaries
- `docs/settlement_domain_types.md`: settlement-domain contract
- `docs/orchestration_runtime.md`: outbox/inbox runtime rules
- `docs/guardrails.md`: executable architectural guardrails
- `docs/happy_route_walkthrough.md`: current Issue #7 end-to-end path
