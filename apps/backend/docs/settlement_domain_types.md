# Settlement Domain Types

This note records the pure settlement-domain contract introduced for M1 Issue #4.

The goal is to give Issue #5 a stable boundary for orchestration work without letting provider logic, database concerns, or PoC wire format shortcuts leak into the domain crate.

## Introduced types

The `musubi_settlement_domain` crate now owns:
- canonical settlement identifiers such as `PaymentReceiptId`, `SettlementCaseId`, `SettlementIntentId`, `SettlementSubmissionId`, and `ObservationId`
- distinct idempotency types: `InternalIdempotencyKey` and `ProviderIdempotencyKey`
- safe money primitives: `CurrencyCode` and `Money`
- backend descriptor, backend pinning, and capability types
- typed provider payload schema/field/value types without pulling serde into the domain crate
- provider-agnostic command, result, observation, and error enums
- settlement state vocabulary: primary phase, resolution kind, overlays, and `SettlementState`
- the pure `SettlementBackend` trait

`SettlementIntentId` is used instead of `PromiseIntentId` because the pinned foundation documents define **Settlement Intent** as the canonical settlement object.

The crate is also split into focused modules so the core contract is readable without one giant `lib.rs`.

## Why money is safe

`Money` uses:
- `i128` minor units
- an explicit `scale`
- an explicit `CurrencyCode`

Arithmetic is only exposed through checked methods:
- `checked_add`
- `checked_sub`
- `checked_cmp`

These methods fail on currency mismatch, scale mismatch, or integer overflow.
There is no unchecked float-based arithmetic and no silent rescaling.

## Why internal and provider idempotency differ

`InternalIdempotencyKey` belongs to MUSUBI orchestration truth.
`ProviderIdempotencyKey` belongs to the backend adapter boundary.

They are intentionally different types so retries cannot accidentally reuse internal identifiers as if they were provider-safe wire values.
The trait exposes provider-key derivation explicitly through `provider_idempotency_key(...)`.

## Why backend pinning is explicit

`BackendPin` carries the pinned `backend_key` and `backend_version` on command objects.
This makes durable replay safer because orchestration can preserve which backend/version a case was pinned to when the command was created.

`SettlementBackend::ensure_backend_pin(...)` fails closed if a command is routed to a backend descriptor that does not match the pinned backend.

## Why provider payload is no longer bytes

The first draft used `ProviderPayload(Vec<u8>)`.
That shape was too permissive and invited future drift toward "just serialize anything."

The payload is now:
- schema-aware
- field-based
- typed through `ProviderPayloadValue`
- still provider-agnostic

This keeps the domain crate free from serde/JSON convenience leakage while still making payload structure explicit.

## Settlement state model

The foundation docs say settlement state is:
- one primary phase
- an optional resolution kind
- zero or more overlays

The crate now reflects that directly with:
- `SettlementPrimaryPhase`
- `SettlementResolutionKind`
- `SettlementOverlay`
- `SettlementState`

## What the trait does

`SettlementBackend` is a pure adapter contract.
It can:
- verify receipts
- submit provider-facing actions
- reconcile provider-observed status
- normalize callbacks into MUSUBI observations
- declare explicit capabilities

It does not:
- own PostgreSQL truth
- write ledger history
- mutate outbox or inbox state
- implement provider networking in this crate

## Intentionally deferred to Issue #5

This issue does not implement:
- transactional outbox / durable inbox runtime
- retry workers or pruning
- provider-specific adapters
- database persistence
- happy-route case progression
- adapter-owned provider payload builders or registries

Those remain downstream of this pure contract layer.
