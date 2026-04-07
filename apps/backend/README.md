# musubi_backend

Minimal Axum backend with:
- `POST /api/auth/pi`
- `POST /api/payment/callback`

## Database skeleton

Issue #3 adds plain SQL migration scaffolding under `migrations/`.
These files establish the Day 1 `core`, `dao`, `ledger`, `outbox`, and `projection` boundaries without adding runtime DB wiring yet.

See `docs/schema_skeleton.md` for ownership notes and deferred scope.
