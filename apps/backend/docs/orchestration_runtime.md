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

## Intentionally deferred

Issue #5 does not implement:
- provider-specific adapters
- queue product selection
- full settlement happy-route behavior
- Issue #6 guardrail tests for broader codebase patterns
- Issue #7 end-to-end Promise or settlement UX
