# Backend Package Boundaries

This note records the Day 1 backend boundary setup introduced for M1 Issue #2.

The goal of this refactor is structural honesty:
- keep the backend runnable as a single Axum app crate
- create clear homes for MUSUBI domain concepts before schema and settlement work expand
- prevent `Server`, `Realm`, `Citadel`, `Pool`, settlement, and orchestration from collapsing into one PoC-shaped module tree

This refactor is intentionally limited.
It does not implement:
- database schema or migrations
- PostgreSQL integration
- settlement backend/provider traits
- outbox/inbox runtime behavior
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

Must not become:
- the long-term home for domain truth
- the settlement architecture
- the place where `Server`, `Realm`, `Citadel`, and `Pool` are blurred together

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

### `musubi_orchestration`
Owns:
- the future boundary for transactional outbox / durable inbox orchestration
- coordination vocabulary only, for now

Must not own:
- real outbox/inbox runtime yet
- database integration
- settlement/provider behavior

## Why this exists

Later M1 issues need stable package boundaries before adding:
- schema skeletons
- settlement-domain core types
- orchestration runtime
- guardrails

This package layout makes those future steps easier without preempting their scope.
