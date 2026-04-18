# Happy Route Walkthrough

This note documents the minimal lawful Day 1 happy route after Issue #21.

The goal is not broad product UX.
It is to prove that the backend can show a MUSUBI-shaped flow with visible truth boundaries:

1. sign-in
2. bounded Promise / deposit intent
3. settlement case creation
4. outbox-driven provider submission
5. observed receipt handling
6. append-only ledger fact creation
7. derived projection update

## Implemented request / response path

### 1. `POST /api/auth/pi`

- Upserts a mutable Ordinary Account envelope in `core.accounts`.
- Upserts the Pi identity link in `core.pi_account_links`.
- Rotates the local bearer session in `core.auth_sessions`.
- Requires a non-empty `access_token` in the request payload.
- Returns a bearer token and stable account id for the signed-in Pi identity.
- Existing `pi_uid` reuse is only allowed when the same access-token fingerprint is presented again; production provider identity verification is still deferred.

### 2. `POST /api/promise/intents`

Request:
- bearer token
- `internal_idempotency_key`
- `realm_id`
- `counterparty_account_id`
- `deposit_amount_minor_units`
- `currency_code`

Writes in one authoritative step:
- `dao.promise_intents`
- `dao.settlement_cases`
- `outbox` message `OPEN_HOLD_INTENT`
- `outbox` message `REFRESH_PROMISE_VIEW`

Immediate response:
- `promise_intent_id`
- `settlement_case_id`
- `case_status = pending_funding`

### 3. `POST /api/internal/orchestration/drain`

This is the explicit demo relay for the local happy route.
It makes the asynchronous boundary visible instead of hiding provider work inside the request that created truth.
The route is mounted in debug builds by default, and in release only when `MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_DRAIN=true`.

For `OPEN_HOLD_INTENT` it:
- claims the outbox row
- inserts command-inbox dedupe for `settlement-orchestrator`
- persists `dao.settlement_intents`
- persists a pending `dao.settlement_submissions`
- drops the authoritative lock
- calls the sandbox Pi `SettlementBackend`
- records the provider idempotency mapping and request hash
- persists submission acceptance + normalized observation in a fresh write
- emits `REFRESH_SETTLEMENT_VIEW`

In code, that split now starts at `process_open_hold_intent(...)` and is explicitly separated into a prepare write and a post-I/O persistence write so the PostgreSQL transaction boundary is explicit and easier to reason about.
The prepare write and post-I/O persistence write are now real PostgreSQL writer transactions.
The sandbox provider call is outside the authoritative transaction.

Immediate response:
- processed outbox rows
- `provider_submission_id` for the accepted hold submission

### 4. `POST /api/payment/callback`

Request:
- `payment_id` = provider submission id from the sandbox Pi backend
- `payer_pi_uid`
- `amount_minor_units`
- `currency_code`
- optional `txid`
- explicit provider `status`

Flow:
- stores `core.raw_provider_callbacks` first, including exact raw body bytes, redacted headers, nullable signature validity, receive time, and dedupe key
- keeps malformed, unmapped, and out-of-order callbacks as raw evidence instead of dropping them in the HTTP request
- records provider callback dedupe evidence from the exact raw payload bytes
- emits `INGEST_PROVIDER_CALLBACK`

Immediate response:
- `raw_callback_id`
- `duplicate_callback`
- `outbox_event_ids`

The callback endpoint does not normalize, verify, fund, append ledger rows, or refresh projections.
It only accepts raw evidence and schedules internal processing.

### 5. `POST /api/internal/orchestration/drain`

For `INGEST_PROVIDER_CALLBACK` it:
- claims the provider callback outbox row
- loads the raw callback evidence
- validates provider submission mapping, amount, and payer only after raw evidence exists
- defers valid callbacks whose provider submission mapping is not ready yet, returning the callback outbox row to retry before manual review is allowed
- normalizes callback evidence through the sandbox Pi backend
- verifies the receipt through the sandbox Pi backend
- writes `core.payment_receipts` idempotently
- advances `dao.settlement_cases` to `funded`
- appends `ledger.journal_entries` + `ledger.account_postings`
- emits `REFRESH_SETTLEMENT_VIEW` and `REFRESH_PROMISE_VIEW`

Exact provider callback replays keep a new raw callback record for evidence, but reuse the existing receipt outcome and do not re-run normalization, verification, ledger append, or projection refresh side effects.

### 6. `POST /api/internal/orchestration/drain`

The following drain work processes projection refresh events and rebuilds:
- `projection.promise_views`
- `projection.settlement_views`

### 7. `GET /api/projection/settlement-views/:settlement_case_id`

Requires a bearer token for an account that participates in the referenced Promise / settlement case.
Returns the derived read model:
- current settlement status
- total funded minor units
- currency code
- latest journal entry id

## Where each truth boundary lives

### Authoritative truth

Written in PostgreSQL through `src/services/happy_route/repository.rs`:
- mutable account/session records
- Promise coordination records
- settlement case / intent / submission / observation records
- payment receipt records
- append-only ledger journals / postings

The happy-route writer uses direct `tokio-postgres` writer connections.
Authoritative writes that enqueue outbox work happen in the same PostgreSQL transaction as their outbox rows.
Provider I/O and callback normalization / verification are not awaited while holding an authoritative transaction open.

### Outbox

Written by:
- `create_promise_intent(...)`
- `process_open_hold_intent(...)`
- `accept_payment_callback(...)`
- `process_provider_callback(...)`

Outbox rows are explicit `OutboxMessageRecord` values with:
- internal event id
- aggregate identity
- schema version
- delivery status
- typed command payload

Rows live in `outbox.events`.
Consumer idempotency lives in `outbox.command_inbox`, with completed rows bounded by retain/prune metadata.

### Evidence observation

Observed in two places:
- `normalize_callback(...)` on inbound callback evidence
- `submit_action(...)` on provider submission acceptance

Both come through the sandbox Pi `SettlementBackend`.
Provider responses are treated as evidence, not business truth.

### Ledger fact append

The happy route appends receipt-recognition truth in PostgreSQL when a verified receipt first funds a settlement case.

It creates:
- one journal entry with `entry_kind = receipt_recognized`
- one debit posting to `provider_clearing_inbound`
- one credit posting to `user_secured_funds_liability`

No historical ledger rows are mutated or deleted.

### Projection update

Projection refresh is handled by outbox consumers:
- `process_refresh_promise_view(...)`
- `process_refresh_settlement_view(...)`

The settlement view total is rebuilt from authoritative ledger postings, not copied from the callback payload.

`GET /api/projection/settlement-views/:settlement_case_id` keeps the existing response contract, but the row is now rebuilt from writer-owned PostgreSQL facts.

## What is real vs stubbed

### Real in this M1 implementation

- explicit Promise -> settlement case authoring
- PostgreSQL writer truth + outbox boundary
- separate outbox relay step
- durable command-inbox dedupe
- sandbox Pi provider submission behind `SettlementBackend`
- durable provider request hash and idempotency mapping
- minimal sandbox provider status polling through `reconcile_submission(...)`
- raw callback first
- durable malformed / unmapped callback evidence retention
- thin callback endpoint that accepts raw evidence and schedules provider callback processing
- out-of-order callback retry while provider submission mapping is still catching up
- raw callback replay detection
- provider error classification into retryable, terminal, and manual-review outcomes
- DB-constrained idempotent payment receipt handling
- append-only ledger journal/posting creation
- projection rebuilt from authoritative truth

### Stubbed on purpose

- production Pi provider / wallet integration
- callback signature verification, until a pinned Pi callback signature / auth contract exists
- production worker deployment and retry tuning beyond the local drain / debug worker
- proof persistence

The backend adapter is sandbox-only, but the truth boundaries are not fake.

## What remains deferred after Issue #21

- move internal relay endpoint behavior into real workers
- add reconciliation paths for unknown / contradictory provider results
- proof persistence
- #22 read-side expansion: trust read models, Promise read models, freshness metadata, rebuild/backfill API contracts, ranking/scoring exclusions, and broader projection growth
- add release / refund / compensation happy and unhappy routes beyond initial funding recognition

## Important Day 1 limitations

- The demo canonicalizes `PI` to scale `3` and uses minor units only.
- The current happy route proves receipt recognition and funding visibility, not full release/refund lifecycle.
- The internal relay endpoint exists for local visibility and tests; it is not the long-term product-facing orchestration surface.
