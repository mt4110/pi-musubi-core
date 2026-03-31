# M1 — Core Truth and Orchestration Baseline

Purpose: Turn `pi-musubi-core` from a Pi Sign-in + deposit PoC into a lawful Day 1 MUSUBI implementation skeleton that cannot easily drift from `musubi-foundation`.

---

## 1. chore: pin musubi-foundation v0.1.0 and wire implementation posture

### Why
Before code evolves, this repository must explicitly acknowledge that `musubi-foundation` is the constitutional source of truth.

### Scope
- add or normalize `AGENTS.md`
- add or normalize `docs/foundation_lock.md`
- update `README.md` with a Foundation alignment section
- pin the implementation repo to foundation `v0.1.0` / `b094727`

### Acceptance criteria
- `AGENTS.md` exists and states that foundation docs outrank local implementation drift
- `docs/foundation_lock.md` exists and is fully pinned
- `README.md` contains a Foundation alignment section
- no app/runtime code is changed by this issue

### Do not
- reinterpret product meaning
- change runtime behavior
- invent new domain vocabulary

### Depends on
- none

---

## 2. refactor: introduce domain-aligned package boundaries for day-1 core

### Why
The implementation repo must not collapse `Server`, `Realm`, `Citadel`, `Pool`, settlement, and coordination into generic SaaS code buckets.

### Scope
Introduce or normalize package / crate boundaries so the codebase can evolve around domain law rather than UI convenience.

Suggested boundaries:
- app/api layer
- core/person/profile layer
- realm/domain layer
- settlement/domain layer
- orchestration/outbox-inbox runtime layer
- projection/read-model layer

### Acceptance criteria
- package or crate layout clearly separates settlement-domain from UI/API glue
- package or crate layout clearly separates realm-domain from settlement-domain
- naming does not collapse `Server`, `Realm`, `Citadel`, or `Pool`
- boundary notes are documented in-code or in local docs

### Do not
- create hidden cross-domain god modules
- place immutable truth and mutable profile logic in the same package by convenience

### Depends on
- `chore: pin musubi-foundation v0.1.0 and wire implementation posture`

---

## 3. feat(db): add schema skeleton for core dao ledger outbox projection

### Why
Physical data separation is a non-negotiable implementation law.

### Scope
Create the initial schema skeleton and migration sequence for:
- `core`
- `dao`
- `ledger`
- `outbox`
- `projection`

The schema must preserve:
- PII separation
- append-only ledger discipline
- outbox/inbox coordination boundaries
- writer-first truth

### Acceptance criteria
- initial migrations exist
- no float money columns are introduced
- immutable financial truth is physically separated from mutable PII-bearing records
- pseudonymous identifiers are used across truth boundaries
- projections are explicitly derivable and non-authoritative

### Do not
- place trust score or balance into mutable user/profile tables by convenience
- merge ledger truth and outbox coordination tables
- rely on replica reads for state-changing decisions

### Depends on
- `refactor: introduce domain-aligned package boundaries for day-1 core`

---

## 4. feat(settlement): add settlement-domain core types and SettlementBackend trait

### Why
The settlement boundary must be provider-agnostic, idempotent, forward-only, and safe for AI-assisted implementation.

### Scope
Introduce core settlement-domain types, including:
- money abstraction with safe arithmetic
- internal vs provider idempotency key separation
- observation/result enums
- backend capability model
- `SettlementBackend` trait

### Acceptance criteria
- no floating-point money types
- money arithmetic is guarded against scale/currency mismatch
- `InternalIdempotencyKey` and `ProviderIdempotencyKey` are distinct
- result/error enums are future-safe
- backend capability checks are explicit

### Do not
- make providers the source of business truth
- put provider-specific logic directly into higher orchestration layers
- implement destructive rollback semantics

### Depends on
- `feat(db): add schema skeleton for core dao ledger outbox projection`

---

## 5. feat(orchestration): implement outbox inbox runtime, retry discipline, and pruning

### Why
A publisher without durable orchestration, idempotent delivery, quarantine handling, and pruning is unfinished and dangerous.

### Scope
Implement:
- transactional outbox publishing
- consumer inbox dedupe
- retry classification
- poison-pill quarantine
- bounded retention / pruning
- external idempotency mapping

### Acceptance criteria
- outbox producer writes happen in the same transaction as authoritative truth changes
- consumers use inbox dedupe
- duplicate delivery is treated as normal
- poison-pill messages can be quarantined
- pruning / archive strategy exists from Day 1
- state-changing decisions read from writer truth, not replicas

### Do not
- hold DB transactions open across external awaits
- assume exactly-once delivery
- let unbounded outbox/inbox growth remain as “future work”

### Depends on
- `feat(settlement): add settlement-domain core types and SettlementBackend trait`

---

## 6. test(guardrails): enforce drop-tx-before-await and writer-first reads

### Why
Architectural laws must be enforced mechanically, not only described in prose.

### Scope
Add tests, guards, or static review hooks that make it harder to reintroduce:
- transaction-held external await
- float money
- replica-based settlement progression
- missing idempotency checks

### Acceptance criteria
- at least one test or guard exists for no-Tx-across-await discipline
- at least one test or guard exists for writer-first state-changing reads
- at least one test or guard exists for idempotency behavior
- failure modes are documented where static enforcement is not possible

### Do not
- rely only on code review memory
- declare this complete without executable checks

### Depends on
- `feat(orchestration): implement outbox inbox runtime, retry discipline, and pruning`

---

## 7. feat(happy-route): implement minimal lawful promise->settlement flow

### Why
After truth boundaries and orchestration exist, the repo needs a minimal end-to-end MUSUBI-aligned flow that demonstrates the architecture in action.

### Scope
Implement the smallest lawful happy route that shows:
- sign-in
- bounded promise/deposit intent
- settlement case creation
- outbox-driven provider submission
- observed receipt handling
- append-only ledger fact creation
- derived projection update

### Acceptance criteria
- the flow uses the canonical domain boundaries
- no hidden distributed transactions are introduced
- provider interaction is adapter-facing, not truth-owning
- objective completion facts can be projected without mutating ledger history
- local demo or test path shows the end-to-end lifecycle

### Do not
- bypass the settlement domain for UI convenience
- reintroduce naive synchronous provider-truth coupling
- ship a happy route that ignores pruning, idempotency, or truth boundaries

### Depends on
- `test(guardrails): enforce drop-tx-before-await and writer-first reads`
