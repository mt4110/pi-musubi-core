# Backend Package Boundaries

This note records the Day 1 backend boundary setup introduced for M1 Issue #2.

The goal of this refactor is structural honesty:
- keep the backend runnable as a single Axum app crate
- create clear homes for MUSUBI domain concepts before schema and settlement work expand
- prevent `Server`, `Realm`, `Citadel`, `Pool`, settlement, and orchestration from collapsing into one PoC-shaped module tree

The original Issue #2 refactor was intentionally limited.
Later milestone work has since added:
- schema skeleton and migrations
- DB runtime and migration runner wiring
- settlement-domain types and backend traits
- orchestration runtime notes

The backend still does not implement:
- provider-specific adapters
- happy-route expansion

## Ownership

### `musubi_backend` app crate
Owns:
- HTTP handlers
- request validation
- runtime bootstrap
- temporary in-memory PoC state wiring
- temporary service glue required to keep the PoC compiling
- wire-format serialization of domain values when the PoC API needs it
- backend startup schema checks through `musubi_db_runtime`

Must not become:
- the long-term home for domain truth
- the settlement architecture
- the place where `Server`, `Realm`, `Citadel`, and `Pool` are blurred together

### `musubi_db_runtime`
Owns:
- writer DB env parsing and conservative connection settings
- migration tracking, checksum validation, and advisory locking
- backend startup schema drift checks
- local-only reset guard

Must not own:
- business truth tables
- provider callbacks
- settlement domain rules
- a fake pool abstraction that implies stronger runtime guarantees than the code provides

### `musubi-ops`
Owns:
- operator/developer CLI commands for local DB bootstrap, migrate, status, and guarded local reset

Must not own:
- application HTTP handlers
- business-domain policy
- production destructive reset behavior

### `musubi_core_domain`
Owns:
- neutral account-adjacent identifiers such as `OrdinaryAccountId`
- core-domain concepts that should stay independent from HTTP and persistence

Must not own:
- PII persistence rules
- realm topology
- settlement logic

### `musubi_realm_domain`
Owns:
- topology vocabulary and identifiers for `Server`, `Realm`, `Citadel`, and `Pool`
- realm-class distinctions such as `shared`, `dedicated`, and `external`

Must not own:
- runtime placement implementation
- settlement concerns
- infrastructure control-plane behavior

### `musubi_settlement_domain`
Owns:
- pure settlement-facing concepts
- minimal identifiers for `Promise`, settlement cases, settlement intents, and payment receipts
- settlement state vocabulary such as primary phase, resolution kind, and overlays
- backend capability declarations and the pure `SettlementBackend` contract
- pure `EscrowStatus`

Must not own:
- JSON / serde wire-format concerns
- provider implementations or callback transport handling
- database code
- runtime/app state

Note:
the current PoC escrow record and callback input remain in the app crate on purpose.
They still encode callback-oriented glue and `f64` PoC data that should not be promoted into long-term domain truth by boundary cleanup alone.
Typed provider payloads in the domain crate must remain provider-agnostic and must not become arbitrary bytes or JSON convenience blobs.

### `musubi_orchestration`
Owns:
- transactional outbox runtime shape
- durable command inbox dedupe shape
- retry classification, quarantine, and pruning policy at the coordination boundary
- writer-first orchestration invariants

Must not own:
- ledger truth
- provider-side settlement behavior
- application HTTP/runtime wiring

## Why this exists

Later M1 issues need stable package boundaries before adding:
- schema skeletons
- settlement-domain core types
- orchestration runtime
- guardrails

This package layout makes those future steps easier without preempting their scope.
