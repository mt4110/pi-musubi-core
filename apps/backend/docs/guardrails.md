# Backend Guardrails

This note records the executable guardrails introduced for M1 Issue #6.

The goal is not fake perfection.
The goal is to make the most failure-prone MUSUBI laws mechanically harder to violate while being honest about what still depends on human review.

## Mechanically enforced now

### 0. CI guardrail sweep

`make guardrail-sweep` runs a deterministic source scan before DB-backed CI
checks. It fails if backend source, backend crates, or migrations introduce:

- floating-point money primitives;
- direct external network client usage outside an explicit reviewed boundary;
- raw `tokio_postgres` transaction surface drift from the reviewed inventory;
- direct outbox / command-inbox hot-table prune/delete surface drift from the
  reviewed inventory;
- production outbox / command-inbox hot-table pruning that deletes before the
  required archive inserts;
- settlement/provider adapter surface drift from the reviewed inventory;
- settlement/provider callsite surface drift from the reviewed inventory;
- public HTTP route, method, and handler surface drift from the reviewed inventory;
- Rust raw-string public route literals that would bypass the ordinary-literal
  inventory scanner;
- internal HTTP route, method, and handler surface drift from the reviewed inventory;
- Rust raw-string `/api/internal/...` route literals that would bypass the
  ordinary-literal inventory scanner;
- nested route/service or split `/api` route-prefix composition that would
  bypass the public/internal route inventories;
- nested `/api/internal` route-prefix composition that would bypass the
  internal route inventory;
- `WriterReadSource::ReadReplica` usage outside the orchestration rejection
  implementation and its tests;
- tracked `.codex` files or a missing `.codex/` ignore rule.

This sweep is intentionally narrow. It is a tripwire for known high-risk drift,
not a complete static proof of architectural correctness.

`make guardrail-sweep-self-test` creates a temporary Git repository with
representative forbidden fixtures and verifies that the sweep patterns detect:

- floating-point money primitives;
- word-boundary network clients such as `reqwest`;
- namespace-style network clients such as `curl::`;
- raw transaction inventory construction;
- coordination hot-table prune/delete inventory construction;
- production archive-before-prune detection for outbox and command-inbox hot
  tables;
- provider adapter inventory construction;
- provider callsite inventory construction;
- method- and handler-aware public HTTP route inventory construction;
- raw-string public route literal detection;
- nested public route/service-prefix detection;
- split public route-prefix detection;
- method- and handler-aware internal HTTP route inventory construction;
- raw-string internal route literal detection;
- split internal route-prefix detection;
- nested internal route-prefix detection;
- `WriterReadSource::ReadReplica` outside its allowlist;
- tracked `.codex` files and `.codex/` ignore-rule detection.

The self-test does not scan production source. It proves that the tripwire
itself has not silently lost detection coverage before the real sweep runs.

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
- code inside the existing reviewed raw transaction inventory
- future service or adapter code that bypasses orchestration helpers entirely

`apps/backend/docs/raw_transaction_inventory.txt` now fixes the current raw
transaction surface by file and matching-line count. New or moved raw
transaction usage must either remove an existing site or update that inventory
deliberately after review. This is not a proof that existing transaction bodies
are safe; it is a CI tripwire against silent surface growth.

`apps/backend/docs/coordination_prune_inventory.txt` fixes the current direct
`DELETE FROM outbox.events` and `DELETE FROM outbox.command_inbox` surface by
file and matching-line count. New or moved hot-table pruning must update that
inventory deliberately after review. This prevents silent growth of direct
coordination pruning surfaces after archive-before-prune landed.

The sweep also checks production source under `apps/backend/src` and
`apps/backend/crates/*/src` for archive-before-prune sequencing on those hot
tables. A production `DELETE FROM outbox.events` must be preceded by inserts
into both `outbox.outbox_event_archive` and `outbox.outbox_attempt_archive`
since the previous event hot-table delete in that file. A production
`DELETE FROM outbox.command_inbox` must be preceded by an insert into
`outbox.command_inbox_archive` since the previous command-inbox hot-table delete
in that file.

This is still a source tripwire, not a SQL parser or proof of semantic
equivalence. Intentional test fixtures that delete outbox rows to simulate PITR
gaps remain covered by the inventory baseline but are not subject to the
production archive-before-prune sequencing check.

`apps/backend/docs/provider_adapter_inventory.txt` fixes the current
settlement/provider adapter surface by file and matching-line count. It
currently allows only the `SettlementBackend` seam and the sandbox Pi adapter in
the happy-route service. New provider adapter types, implementations, or hook
methods must update that inventory deliberately after review. This prevents
silent provider-boundary growth; it does not prove adapter correctness or
provider guarantee semantics.

`apps/backend/docs/provider_callsite_inventory.txt` fixes the current
production provider callsite surface by file and matching-call count for
method-call and UFCS-style calls to `submit_action`, `verify_receipt`,
`reconcile_submission`, and `normalize_callback`. Same-line calls count
separately, and the UFCS matcher is method-name based so imported trait aliases
do not silently bypass the inventory. New or moved provider callsites must
update that inventory deliberately after review. This prevents silent growth of
external-I/O-shaped provider calls outside the reviewed happy-route boundaries;
it does not prove that an allowed callsite is outside a live database
transaction.

`apps/backend/docs/public_route_inventory.txt` fixes the current production
public HTTP route surface by source file, route literal, HTTP method, and
handler path for `/health` and non-internal `/api/...` routes. New, removed,
moved, method-expanded, or handler-changed public routes must update that
inventory deliberately after review. Because Axum `get(...)` also exposes
`HEAD`, the inventory records implicit `HEAD` alongside `GET` with the same
handler. If the scanner cannot infer a method, it records `UNKNOWN_METHOD`
instead of silently ignoring the route. If it cannot infer a simple handler
path, it records `UNKNOWN_HANDLER`. This prevents silent growth or same-surface
handler replacement across user-facing launch, auth, Promise, proof,
projection, review, realm, payment, and health surfaces; it does not prove that
an allowed public route has the right launch gate, consent gate, body limit,
redaction behavior, or writer-truth semantics. The sweep also forbids
production source from composing Rust raw-string public route literals or
nested route/service and split `/api` route-prefix literals, so public routes
must remain visible as ordinary full route literals in the inventory.

`apps/backend/docs/internal_route_inventory.txt` fixes the current production
`/api/internal/...` HTTP route surface by source file, route literal, HTTP
method, and handler path. New, removed, moved, method-expanded, or
handler-changed internal routes must update that inventory deliberately after
review. Because Axum `get(...)` also exposes `HEAD`, the inventory records
implicit `HEAD` alongside `GET` with the same handler. If the scanner cannot
infer a method, it records `UNKNOWN_METHOD` instead of silently ignoring the
route. If it cannot infer a simple handler path, it records `UNKNOWN_HANDLER`.
This prevents silent growth or same-surface handler replacement across
operator, repair, drain, rebuild, and observability surfaces; it does not prove
that an allowed internal route has the right auth gate, release gate, body
limit, redaction behavior, or writer-truth semantics.
The sweep also forbids production source from composing Rust raw-string internal
route literals, an exact `"/api/internal"` nest prefix, or nested route/service
and split `/api` route-prefix literals, so internal routes must remain visible
as ordinary full `"/api/internal/..."` literals in the inventory.

So the rule is still:
- keep authoritative transaction code database-only
- perform provider/network I/O only after that transaction is committed or dropped
- treat every internal HTTP surface as an explicit operator/review boundary,
  not a convenience route

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
- turn the archive-before-prune source tripwire into a syntax-aware lifecycle
  lint once coordination lifecycle shapes grow beyond the current SQL helpers
- turn the internal route inventory into route-metadata linting once gate/auth
  declarations are centralized
- turn the provider adapter and callsite inventories into a richer boundary lint once production Pi networking is pinned

## Bottom line

Today the repo has real guardrails for:
- writer-first progression
- duplicate-delivery / duplicate-submission behavior
- async boundary ordering around outbox and inbox work

And it is still honest that some no-Tx-across-await violations remain review-detectable rather than fully statically impossible.
