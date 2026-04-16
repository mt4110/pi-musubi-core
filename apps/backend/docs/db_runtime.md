# Backend DB Runtime

This note records the Issue 08 runtime spine for local PostgreSQL bootstrap, deterministic migrations, and backend startup checks.

## Authority

PostgreSQL is the writer truth boundary. Provider callbacks, projections, caches, and UI state remain evidence or derived data. The migration runner only creates and verifies schema; it does not make external calls and it does not hold a database transaction across network I/O.

## Environment Contract

Required:
- `APP_ENV`: one of `local`, `test`, `staging`, or `prod`
- `DATABASE_URL`: writer PostgreSQL URL

Optional defaults:
- `DATABASE_MIN_CONNECTIONS=2`
- `DATABASE_MAX_CONNECTIONS=16`
- `DATABASE_ACQUIRE_TIMEOUT_MS=3000`
- `DATABASE_STATEMENT_TIMEOUT_MS=5000`
- `DATABASE_IDLE_TIMEOUT_MS=30000`
- `REQUIRE_LATEST_SCHEMA=true`
- `MIGRATIONS_DIR=./migrations`

The pool values are explicit runtime contract values. The current implementation uses direct writer connections for migration and startup checks so it does not invent a weak pool abstraction before the app has real concurrent DB usage.

## Local Flow

```bash
cd apps/backend
cp .env.example .env
docker compose up -d postgres redis
make db-bootstrap
make db-migrate
make db-status
make dev
```

Equivalent raw commands:

```bash
cargo run -p musubi-ops -- db bootstrap
cargo run -p musubi-ops -- db migrate
cargo run -p musubi-ops -- db status
cargo run -p musubi_backend
```

Backend startup connects to the writer DB and checks migration status. When `REQUIRE_LATEST_SCHEMA=true`, startup fails if migration tracking is missing, the DB has an applied migration missing from the local checkout, a migration failed, checksum drift exists, or pending migrations remain.

The happy-route integration tests use `MUSUBI_TEST_DATABASE_URL` as their writer truth database.
The test harness serializes DB-backed app states, bootstraps / migrates the test DB once per test process, and truncates the happy-route truth, outbox, ledger, and projection tables before each test.

## Migration Rules

- SQL files live under `apps/backend/migrations/`.
- File names must be ordered, such as `0001_create_core_schema.sql`.
- Checksums are recorded in `public.musubi_schema_migrations`.
- `db migrate` takes a PostgreSQL advisory lock and exits if another runner holds it.
- `db status` reports whether the migration advisory lock is currently available.
- `db status` reports applied DB migrations that are missing from the local checkout as `unexpected applied`; this blocks startup and later migration attempts.
- Failed migration attempts are recorded and block later migration attempts until handled.
- Applied migrations must not be edited. Add a new migration instead.

## Local Reset

`db reset-local` is intentionally hard to run:

```bash
cargo run -p musubi-ops -- db reset-local --confirm-reset-local
```

It is refused unless:
- `APP_ENV=local`
- `DATABASE_URL` points at `localhost`, `127.0.0.1`, `::1`, or the local compose host `postgres`
- the confirmation flag or `MUSUBI_CONFIRM_RESET_LOCAL=reset-local` is present

Do not bypass this guard for staging or production.
