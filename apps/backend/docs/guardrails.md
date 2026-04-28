# Backend Guardrails

This note records the executable guardrails introduced for M1 Issue #6.

The goal is not fake perfection.
The goal is to make the most failure-prone MUSUBI laws mechanically harder to violate while being honest about what still depends on human review.

## Mechanically enforced now

### 1. Writer-first state-changing reads

The orchestration boundary encodes writer-only progression through `WriterReadSource`.

Current executable guards:
- `InMemoryOrchestrationStore` rejects `WriterReadSource::ReadReplica` for:
  - `commit_authoritative_write(...)`
  - `claim_ready_outbox(...)`
  - `begin_command(...)`
- `OrchestrationRuntime` does not expose a replica option for those progression paths. It always uses `WriterReadSource::PrimaryWriter`.
- `apps/backend/crates/orchestration/tests/runtime_contract.rs` includes `authoritative_progression_rejects_read_replica_sources`.

Why this matters:
- settlement progression
- retry/claim progression
- command dedupe progression

These are the places where replica lag would create duplicate execution or stale settlement truth.

### 2. Idempotency behavior

Current executable guards:
- producer-side duplicate outbox idempotency keys are rejected before a second authoritative change is recorded
- consumer-side duplicate command delivery is treated as normal and returns `ConsumeOutcome::Duplicate`
- command payload drift for the same `(consumer_name, command_id)` is rejected

Current executable tests:
- `duplicate_outbox_idempotency_key_does_not_duplicate_truth`
- `duplicate_consumer_delivery_is_a_no_op`
- `postgres_helpers_keep_truth_and_outbox_in_same_transaction`

This is intentionally database-shaped logic, not in-memory optimism.

### 3. Drop-Tx-Before-Await at the runtime seam

Current executable guards:
- `OrchestrationRuntime::deliver_ready_outbox(...)` claims work first, then awaits publish, then records the result in a fresh store call
- `OrchestrationRuntime::consume_command(...)` begins inbox processing first, then awaits the handler, then records completion/retry/quarantine in a fresh store call
- `apps/backend/crates/orchestration/tests/runtime_contract.rs` includes:
  - `outbox_publish_callback_runs_between_writer_phases`
  - `command_handler_runs_between_writer_phases`

What those tests prove:
- the async callback runs after the writer-side claim/begin step
- the completion write happens only after the callback future resolves
- later refactors cannot silently move the completion write ahead of the external await without breaking tests

## Still review-only for now

### 1. Raw transaction code outside orchestration/runtime seams

`PostgresOrchestrationStore::record_authoritative_write(...)` no longer accepts an arbitrary async closure.
It now accepts only declarative SQL commands plus the outbox message, and it rejects an empty command batch.
That removes the easiest place to smuggle remote `await` into a live authoritative transaction and also prevents "outbox only" use through that helper.

What still remains review-sensitive:
- code that takes a raw `tokio_postgres::Transaction<'_>` directly
- future service or adapter code that bypasses orchestration helpers entirely

So the rule is still:
- keep authoritative transaction code database-only
- perform provider/network I/O only after that transaction is committed or dropped

### 2. Code outside the orchestration crate

The current tests guard the orchestration boundary.
They do not automatically police every future Axum handler, service, or provider adapter that may open its own transaction.

If later code bypasses orchestration and performs:
- authoritative DB write
- remote await
- same-tx follow-up mutation

that would still be a bug even if these tests stay green.

## Why static enforcement is limited today

The backend is still on a small Day 1 skeleton:
- no custom lint crate
- no MIR/static analysis pass
- only a sandbox provider adapter, with production Pi networking still deferred

Because of that, the most honest posture is:
- encode the invariant at the runtime seam with executable sequencing tests
- narrow the PostgreSQL helper so it cannot accept arbitrary async callback logic
- encode writer-first via interface and tests
- encode idempotency via durable uniqueness and duplicate-delivery tests
- explicitly document the places where review is still required

Issue #17 adds the first sandbox Pi provider adapter in the happy-route service.
It follows the same no-transaction-across-provider-await shape by preparing authoritative state, releasing the store lock, calling the adapter, and persisting the result in a later write.
Provider errors now keep a retry class at the app boundary: transient provider failures can retry, valid out-of-order callbacks defer while provider submission mapping catches up, terminal failures are quarantined, and ambiguous provider behavior is held for manual review instead of being returned to the pending queue.
Payment callbacks now persist exact raw body bytes and redacted headers before mapping, amount, payer, normalization, or receipt verification logic runs.
The HTTP callback endpoint does not advance settlement state, append ledger rows, or refresh projections; it schedules `INGEST_PROVIDER_CALLBACK` for outbox-driven orchestration.

Issue #16 adds a writer-owned orchestration repair pass for recovery, PITR asymmetry, and stale claim cleanup.
The repair path is internal-only, separated from the drain gate, records bounded `outbox.recovery_runs`, reads writer truth only, and treats projection or observability rows as non-authoritative.
It requires an explicit dry-run/scope/reason body plus operator id, takes a transaction-scoped advisory lock, and caps each repair category with deterministic ordering.
It repairs forward by resetting expired coordination claims for known orchestration types while re-checking stale predicates at mutation time, closing completed-consumer/unfinished-producer gaps only when command identity and checksum still match, re-enqueuing lost callback-ingest coordination only from accepted writer-owned settlement evidence with matching payer/amount/currency, and applying verified receipt side effects without duplicating receipt-recognition journals or advancing incomplete ledger posting state.

ISSUE-10 adds safer venue proof input primitives.
Proof challenges are short-lived, near single-use, account-bound, realm/venue-bound, and verified against the challenge-issued, server-secret-backed, key-version-aware rotating code or a rate-limited operator fallback.
The subject-facing proof challenge route only supports the normal venue-code flow; `operator_pin` is rejected before service-layer issuance so subject callers cannot self-assert operator identity, create operator audit rows, or burn operator fallback rate-limit budget.
The proof service records only bounded proof-input evidence: sanitized coarse location hints, hashed device-session hints, server-keyed display-code hashes, canonical fallback modes, server-keyed replay keys, and redacted payload shape.
Failed-attempt budget is charged only for bound secret-check failures after challenge, account, venue, nonce, and live-status binding.
It does not write raw GPS, exact addresses, oversized location strings, arbitrary fallback-mode strings, clear display codes, clear operator PINs, or excessive device fingerprint material into the current in-memory truth stand-in.
Venue display codes and operator PIN hashes are not derived from public identifiers alone.
Verified proof remains an input fact, not authoritative business truth.
Product and later domain flows must not describe this as complete anti-spoofing.

## Expected next improvements

The next meaningful upgrades would be:
- add integration tests around real PostgreSQL writer/claim/persist flows as the happy route grows
- add CI review hooks or linting that flag suspicious `Transaction` + remote client usage patterns
- keep new settlement/provider code inside orchestration and settlement boundaries rather than ad hoc service methods

## Bottom line

Today the repo has real guardrails for:
- writer-first progression
- duplicate-delivery / duplicate-submission behavior
- async boundary ordering around outbox and inbox work

And it is still honest that some no-Tx-across-await violations remain review-detectable rather than fully statically impossible.
