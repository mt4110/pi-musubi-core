# musubi_backend

Day 1 backend app and Rust workspace root.

Current local HTTP surface:
- `POST /api/auth/pi` with explicit `access_token`
- `POST /api/promise/intents`
- `POST /api/payment/callback`
- `POST /api/proof/challenges`
- `POST /api/proof/submissions`
- `POST /api/internal/orchestration/drain` in debug builds, or in release only when `MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_DRAIN=true` and the request includes `Authorization: Bearer $MUSUBI_INTERNAL_API_TOKEN`
- `GET /api/internal/ops/health` under the same internal/debug gate; reports DB connectivity only
- `GET /api/internal/ops/readiness` under the same internal/debug gate; reports migration posture without mutating migration state
- `GET /api/internal/ops/observability/snapshot` under the same internal/debug gate; returns a redacted ops snapshot without raw evidence, operator notes, source identifiers, or participant data
- `GET /api/internal/ops/observability/slo` under the same internal/debug gate; aliases the redacted SLO snapshot
- `POST /api/internal/projection/rebuild` under the same internal/debug gate and release-time internal bearer-token requirement
- `POST /api/internal/operator/review-cases` under the same internal/debug gate; requires `x-musubi-operator-id` with a durable operator role grant
- `GET /api/internal/operator/review-cases` under the same internal/debug gate; requires `x-musubi-operator-id` with a durable operator role grant
- `GET /api/internal/operator/review-cases/{review_case_id}` under the same internal/debug gate; returns bounded operator case detail without raw evidence locators or internal notes
- `POST /api/internal/operator/review-cases/{review_case_id}/evidence-bundles` under the same internal/debug gate
- `POST /api/internal/operator/review-cases/{review_case_id}/evidence-access-grants` under the same internal/debug gate
- `POST /api/internal/operator/review-cases/{review_case_id}/decisions` under the same internal/debug gate; appends operator decision facts instead of rewriting source truth
- `POST /api/review-cases/{review_case_id}/appeals` for the authenticated review subject
- `GET /api/review-cases/{review_case_id}/appeals` for the authenticated review subject
- `GET /api/review-cases/{review_case_id}/status` for the authenticated review subject
- `POST /api/realms/requests` for authenticated realm request creation
- `GET /api/realms/requests/{realm_request_id}` for the authenticated requester
- `GET /api/projection/realms/{realm_id}/bootstrap-summary` for the authenticated requester or admitted participant; returns bounded realm/admission state without operator IDs or internal review details
- `POST /api/internal/room-progressions` under the same internal/debug gate
- `POST /api/internal/room-progressions/{room_progression_id}/facts` under the same internal/debug gate
- `POST /api/internal/projection/room-progressions/rebuild` under the same internal/debug gate
- `GET /api/internal/operator/realms/requests` under the same internal/debug gate; requires `x-musubi-operator-id` with a durable operator role grant
- `GET /api/internal/operator/realms/requests/{realm_request_id}` under the same internal/debug gate; returns operator review detail for the request
- `POST /api/internal/operator/realms/requests/{realm_request_id}/approve` under the same internal/debug gate; approves a bounded bootstrap or active realm from a request
- `POST /api/internal/operator/realms/requests/{realm_request_id}/reject` under the same internal/debug gate
- `POST /api/internal/realms/{realm_id}/sponsor-records` under the same internal/debug gate; writes explicit sponsor authority with quota
- `POST /api/internal/realms/{realm_id}/admissions` under the same internal/debug gate; derives admission kind from writer truth, sponsor status, and corridor state
- `GET /api/internal/operator/realms/{realm_id}/review-summary` under the same internal/debug gate; returns redacted bootstrap health state
- `POST /api/internal/projection/realms/rebuild` under the same internal/debug gate
- `GET /api/projection/room-progression-views/{room_progression_id}` for authenticated room participants only
- `GET /api/projection/settlement-views/{settlement_case_id}` for authenticated participants only
- `GET /api/projection/settlement-views/{settlement_case_id}/expanded` for authenticated participants only
- `GET /api/projection/promise-views/{promise_intent_id}` for authenticated participants only
- `GET /api/projection/trust-snapshots/{account_id}` self-scoped
- `GET /api/projection/realm-trust-snapshots/{realm_id}/{account_id}` self-scoped and realm-local

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
Issue #22 adds derived Promise, expanded settlement, and bounded trust read models with rebuild and freshness metadata.
Design source: ISSUE-12-operator-review-appeal-evidence.md adds the operator review / appeal / evidence workflow baseline. Operator decisions are append-only facts, original writer truth is not overwritten, concurrent idempotent replays return the preserved case or fact instead of surfacing a duplicate-write error, replay mismatches are checked with payload hashes, legacy rows backfill missing replay hashes on first retry, and user-facing review state is projected with bounded status and reason codes. See `docs/operator_review_workflow.md`.
Design source: ISSUE-13-room-progression.md adds the room progression surface baseline. Room progression facts are append-only writer facts in `dao`, user-facing room state is rebuilt into `projection`, sealed fallback can link to ISSUE-12 review cases without duplicating review/evidence truth, and state-changing progression decisions read writer truth instead of projection rows. See `docs/room_progression_surface.md`.
Design source: ISSUE-15-realm-bootstrap-and-admission.md adds the first bounded realm bootstrap and admission baseline. Realm creation requests, sponsor records, bootstrap corridors, admissions, and review triggers live in writer-owned `dao` tables, while participant-safe and operator-safe realm summaries stay rebuildable in `projection`. Sponsor quota, revoked/rate-limited sponsor state, corridor expiry/caps, and restricted/suspended realm blocking are enforced on the writer path, not by client/UI state or projection rows. See `docs/realm_bootstrap_surface.md`.
Design source: ISSUE-17-observability-slo-prompt-pack adds an internal-only, read-only ops observability surface. Health, readiness, projection lag, review queue, Realm review-trigger, and orchestration backlog signals are reported as redacted operational posture only. Unsupported SLIs return `unknown` instead of fake zero values, and projection lag remains display-only rather than writer truth. See `docs/ops_observability_slo.md`.
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
- `docs/projection_read_models.md`: derived read-side contracts, rebuild path, and bounded trust boundary
- `docs/operator_review_workflow.md`: ISSUE-12 operator review, appeal, evidence, and append-only decision boundary
- `docs/room_progression_surface.md`: ISSUE-13 Intent / Coordination / Relationship / Sealed Room progression surface boundary
- `docs/realm_bootstrap_surface.md`: ISSUE-15 bounded realm bootstrap, admission, sponsor, and corridor boundary
- `docs/ops_observability_slo.md`: internal-only ISSUE-17 observability SLO skeleton and redaction boundary
- `docs/guardrails.md`: executable architectural guardrails
- `docs/proof_primitives.md`: Day 1 safer venue proof input boundary
- `docs/happy_route_walkthrough.md`: current Issue #7 end-to-end path
