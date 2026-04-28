# Orchestration Runtime

This note records the Day 1 orchestration runtime introduced for M1 Issue #5.

The goal is to make MUSUBI's coordination discipline explicit before Issue #6 adds guardrails and before Issue #7 adds more happy-route behavior.

## Producer rule

Authoritative truth changes and outbox writes are coupled through one writer-transaction boundary in `musubi_orchestration::OrchestrationRuntime::record_authoritative_write(...)`.

The runtime does not allow:
- authoritative change first, publish later in memory
- publish first, truth later
- replica reads to decide progression

The store contract names this explicitly:
- producer-side progression methods require `WriterReadSource::PrimaryWriter`
- the same store call persists the authoritative change and the durable outbox envelope
- the Postgres helper `PostgresOrchestrationStore::record_authoritative_write(...)` keeps the authoritative write and the outbox insert inside one database transaction

## Consumer inbox dedupe

Consumers use `command_inbox` as the durable dedupe boundary.
The durable dedupe key is `consumer_name + command_id`.
`source_event_id` remains correlation evidence, not the uniqueness boundary.

The runtime shape is:
1. insert or claim the inbox row on the writer
2. if the command is already completed or actively claimed, treat redelivery as normal
3. drop the write transaction
4. run the handler
5. persist completion, retry scheduling, or quarantine in a fresh write

This keeps duplicate delivery normal without pretending exactly-once exists.

## Retry classification

The runtime distinguishes:
- `Transient`
- `Permanent`
- `Deferred`

`Transient` schedules deterministic exponential backoff with deterministic jitter derived from the event or command identity.

`Permanent` and exhausted retries do not loop forever.
They move into visible quarantine.

`Deferred` exists for bounded compatibility windows such as unknown schema during rolling deployment.
The runtime applies this before invoking handlers so unknown future schema does not depend on ad hoc consumer code.

## Poison-pill quarantine

Poison pills are durable, visible failures.

The runtime quarantines:
- malformed or unsupported payloads
- permanent processing failures
- transient work that exceeds the bounded retry budget

Quarantine is stored on the outbox/inbox record itself and is not treated as success.
Other independent work can continue.

## External idempotency mapping

External publish success requires an explicit external idempotency key.

The runtime does not treat publish success without that mapping as a valid completion shape.
This keeps at-least-once delivery aligned with downstream dedupe instead of relying on best effort.

## Pruning strategy

Day 1 pruning is real, not deferred.

The runtime provides `prune_coordination(...)`, and the SQL layer adds:
- `outbox.outbox_event_archive`
- `outbox.outbox_attempt_archive`
- `outbox.command_inbox_archive`

The intended flow is:
1. terminal outbox or inbox rows receive `retain_until`
2. a manual or scheduled runner archives summary data
3. the hot coordination row is pruned

This keeps dedupe and replay evidence long enough to be useful without letting coordination tables become eternal truth.

## Writer-first invariant

The repository does not yet have replica plumbing.
Because of that, the orchestration store only exposes state-changing reads through a `WriterReadSource::PrimaryWriter` contract, and it rejects replica reads when a progression decision is attempted.

That keeps the invariant explicit now instead of relying on code review memory later.

## Recovery and reconciliation baseline

Issue #16 adds an internal repair pass at `POST /api/internal/orchestration/repair`.
It is mounted behind a separate internal/debug gate from drain.
In debug builds it is available unless `MUSUBI_DISABLE_INTERNAL_ORCHESTRATION_REPAIR=true`.
In release builds, `MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_DRAIN=true` does not expose repair; repair requires `MUSUBI_ENABLE_INTERNAL_ORCHESTRATION_REPAIR=true`.
In release builds it requires `Authorization: Bearer $MUSUBI_INTERNAL_API_TOKEN`.

Repair requires an explicit JSON request body:
- `dry_run`
- `reason`
- `max_rows_per_category` between 1 and 500
- `include_stale_claims`
- `include_producer_cleanup`
- `include_callback_ingest`
- `include_verified_receipt_side_effects`

At least one include flag must be true.
Each run requires `x-musubi-operator-id`, caps the trimmed reason at 1000 characters, caps the operator id at 200 characters, takes a transaction-scoped advisory lock, records the operator, reason, requested scope, dry-run flag, row bound, and whether the result was limited.
Dry runs record the audit row and count bounded writer-owned targets, but do not mutate domain or coordination rows.
The HTTP body is limited to 16 KiB.

The repair pass uses the writer database as the only authority.
It records each run in `outbox.recovery_runs` and does only forward repair:
- reset expired `processing` outbox leases back to `pending` only for known orchestration event types
- reset expired `processing` command-inbox leases back to `pending` only for known event/consumer pairs
- mark an event `published` when the matching expected consumer command is already `completed`, with matching command id, source event id, command type, schema version, and payload checksum, but producer cleanup did not finish
- re-enqueue `INGEST_PROVIDER_CALLBACK` only when raw callback evidence still matches an accepted writer-owned settlement submission, successful callback status, expected payer, expected amount, expected currency, and no verified receipt or ingest event already exists
- apply verified receipt side effects when a verified writer-owned receipt exists but the settlement case or receipt-recognition journal side effect was not completed; an existing receipt-recognition journal is considered repairable only when its canonical debit and credit postings are already complete

Recovery-run audit rows carry `retain_until`, and each repair pass prunes expired rows plus old completed audit rows beyond the hot-table retention cap.
They are operational evidence with bounded retention, not eternal truth.

Projection rows and observability snapshots are not repair authority.
They may show stale or misleading state after failover or PITR restore, but recovery decides from writer-owned `dao`, `core`, `ledger`, and `outbox` records.
Repair never destructively rewrites authoritative history; duplicate repair runs must converge through database idempotency and existing writer truth.

## Intentionally deferred

Issue #5 does not implement:
- provider-specific adapters
- queue product selection
- full settlement happy-route behavior
- Issue #6 guardrail tests for broader codebase patterns
- Issue #7 end-to-end Promise or settlement UX
