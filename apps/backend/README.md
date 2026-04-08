# musubi_backend

Minimal Axum backend with:
- `POST /api/auth/pi`
- `POST /api/payment/callback`

## Local infra

The backend now ships with a minimal local development stack:
- PostgreSQL for authoritative business truth
- Redis for future cache / queue / coordination work

### Start local infra

```bash
cd apps/backend
cp .env.example .env
docker-compose up -d postgres redis
```

### Run the backend on the host

```bash
cd apps/backend
cargo run
```

`DATABASE_URL` points at the development database on `127.0.0.1:55432`.
`MUSUBI_TEST_DATABASE_URL` points at the test database on `127.0.0.1:55432`.
`REDIS_URL` points at the local Redis instance on `127.0.0.1:56379`.

### Run the orchestration contract tests

```bash
cd apps/backend
set -a
. ./.env
set +a
cargo test -p musubi_orchestration
```

## Database skeleton

Issue #3 adds plain SQL migration scaffolding under `migrations/`.
These files establish the Day 1 `core`, `dao`, `ledger`, `outbox`, and `projection` boundaries without adding runtime DB wiring yet.

See `docs/schema_skeleton.md` for ownership notes and deferred scope.
