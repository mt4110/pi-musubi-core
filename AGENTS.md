# AGENTS.md

Status: Draft v0.1
Applies to: `mt4110/pi-musubi-core`
Purpose: This repository is the canonical implementation repository for MUSUBI Day 1. It must implement the MUSUBI foundation documents faithfully, not reinterpret them.

---

## 0. Role of this repository

This repository is **the implementation repo**, not the constitutional source-of-truth repo.

- `musubi-foundation` = law, terminology, architecture, whitepaper, diagrams
- `pi-musubi-core` = executable implementation of those laws

If code in this repository conflicts with the pinned foundation documents, **the foundation documents win**.

This repository currently starts from a Day 1 web PoC with:

- `apps/mobile` = Flutter Web frontend
- `apps/backend` = Rust / Axum backend

Day 1 product posture is:

- **web-only**
- **Pi App / DApp first**
- **not a standalone native iOS / Android dating app**

---

## 1. Read this before doing any work

Before changing code, the agent MUST read in this order:

1. `docs/foundation_lock.md`
2. pinned upstream documents listed there
3. this file (`AGENTS.md`)
4. local repository README and code

If a required upstream document is missing locally, stop and escalate rather than guessing.

### 1.1 Refresh integration branches before issue work

If an issue prompt tells the agent to branch from an integration branch such as `feat/happy_route`, the agent MUST treat branch freshness as part of setup.

Before cutting the issue branch:

1. `git fetch origin --prune`
2. fast-forward local `main` to `origin/main`
3. check whether `origin/main` is already an ancestor of the integration branch
4. if not, update the integration branch from `origin/main` before cutting the issue branch, or stop and escalate if that merge is not intended

If a required design note exists on `main` but is missing on the integration branch, treat that as a branch-freshness problem, not as permission to guess.

---

## 2. Mandatory implementation posture

### 2.1 Implement the constitution, do not reinterpret it
This repo exists to implement MUSUBI faithfully.
Do not "improve" the product by drifting toward generic social-app defaults.

### 2.2 Calm product over engagement addiction
Do not introduce features that optimize for:

- infinite swiping
- open inboxes by default
- engagement loops based on anxiety or status
- pay-to-win reach
- paid romantic advantage
- DM unlock by payment
- hot-or-not mechanics
- attention extraction as core business model

### 2.3 Web-only Day 1
Do not assume a native mobile app architecture, native push-notification dependency, or App Store / Play Store-specific monetization model as the primary shape of the product.

### 2.4 Code must preserve domain boundaries
The following boundaries are architectural law:

- `Server` = user-facing alias / discovery surface
- `Realm` = durable logical community boundary
- `Citadel` = runtime placement / operating unit
- `Pool` = shared operational substrate

Do not collapse these concepts in code, schema, naming, or API design.

---

## 3. Canonical terms that must not drift

If code needs these concepts, use the canonical names from foundation documents.
Do not casually replace them with generic SaaS words.

Must preserve:

- Server
- Realm
- Citadel
- Pool
- realm_id
- Ordinary Account
- Controlled Exceptional Account
- Social Trust
- Relationship Depth
- Promise
- Steward
- KASHI (internal canonical name)
- O-Mise
- Intent Room
- Coordination Room
- Relationship Room
- Sealed Room

If a new durable noun is needed and it does not exist in `glossary.md`, stop and escalate.
Do not silently invent policy-bearing vocabulary.

---

## 4. Hard implementation rules

### 4.1 PostgreSQL is business truth
Implement PostgreSQL as the authoritative business source of truth.
Providers, chains, external callbacks, and realm-local runtimes are observed evidence, not truth.

### 4.2 Physical schema separation is mandatory
Maintain hard separation between:

- mutable PII-bearing core records
- immutable / append-only ledger truth
- settlement / dao coordination records
- outbox / inbox coordination logs
- projections / read models

Never store raw PII directly inside immutable ledger or coordination payloads if it can be avoided.
Use pseudonymous identifiers and segregated core records.

### 4.3 No hidden distributed transactions
Do not implement cross-boundary writes as a single synchronous transaction across:

- global state and realm-local state
- database state and provider I/O
- producer completion and consumer completion

Use append-only records, inbox/outbox, and eventual consistency.

### 4.4 Drop-Tx-Before-Await
Never hold an authoritative database transaction open while awaiting external network I/O.
This applies to both backend adapters and orchestrators.

### 4.5 No float money
Never represent money, balances, escrow, or reward quantities with floating-point types.
Use integer minor units or fixed-point decimal abstractions only.

### 4.6 Database-enforced idempotency
Idempotency must be enforced by durable database constraints and records, not only by in-memory checks.
Treat duplicate delivery as normal.

### 4.7 Forward-only compensation
Failures are repaired with new facts.
Do not implement destructive rollback of authoritative financial history.

### 4.8 Writer-first state-changing reads
Any read that decides settlement progression, reward progression, or safety-critical state transitions must read from the writer / primary truth source, not a lagging replica.

### 4.9 Outbox / inbox must be pruned
Outbox and inbox are coordination data, not eternal truth.
Implement bounded retention, archive, or pruning from Day 1.
Do not leave cleanup as “later”.

### 4.10 Objective facts outrank narrative
When the domain requires trust-sensitive updates, prefer objective system facts (escrow release, attested venue proof, bounded verified completion) over purely retaliatory narratives.

---

## 5. Domain-specific red lines

### 5.1 Trust is not vanity
Do not model trust as popularity, follower count, or raw engagement.
Trust is reliability infrastructure.

### 5.2 Trust never overrides consent
High trust must not bypass consent, speed-gate progression, or weaken blocking / withdrawal.

### 5.3 Promise is not access to a person
A Promise is a bounded accountable commitment, not a right to someone’s time, body, or attention.

### 5.4 Escrow is discipline, not bounty
Forfeited escrow must not create bounty-scam incentives.
It is not personal loot for counterparties and not casual revenue extraction for the platform.

### 5.5 O-Mise is not a human courtroom
Venues may serve as passive proof anchors.
Do not design flows that deputize venue staff as human referees.

### 5.6 Off-platform handoff is a trust transition
Do not prevent off-platform movement via surveillance or keyword censorship.
The in-platform advantage is protection, accountable completion, and trust accrual.

---

## 6. Day 1 anti-footgun priorities

When implementing first milestones, prioritize these four anti-footgun constraints:

1. **PII segregation from immutable truth**
2. **Drop-Tx-Before-Await**
3. **Outbox / inbox pruning from Day 1**
4. **Hard-to-fake real-world proof inputs**

Day 1 “check-in” or “proof” APIs must not trust naive GPS alone or static QR alone.
Assume client inputs are spoofable.
Prefer dynamic signed venue challenges, bounded attestations, or similarly spoof-resistant inputs.

---

## 7. What the agent should build first

Unless the task explicitly says otherwise, implementation should proceed in this rough order:

1. foundation lock and repo wiring
2. crate / package boundaries
3. schema skeleton and migrations
4. settlement-domain core types
5. outbox / inbox and orchestration primitives
6. proof / venue / promise primitives
7. minimal end-to-end happy route
8. hardening, tests, pruning, reconciliation, and review tooling

---

## 8. When to stop and escalate

Stop and escalate if the requested implementation would:

- conflict with `musubi-foundation`
- introduce paid romantic advantage
- introduce DM unlock by payment
- merge PII into immutable ledger truth
- require hidden distributed transactions
- treat trust as popularity or human worth
- weaken one-person-one-ordinary-account
- bypass consent because of trust or operator status
- assume native mobile as the primary runtime
- invent new durable domain vocabulary not in foundation docs

Also stop and escalate if:

- a migration would require destructive truth-table rewriting
- a feature depends on unresolved provider guarantees
- a design needs governance or constitutional decisions not yet pinned

---

## 9. Done means

A task is not done unless:

- implementation follows the pinned foundation docs
- terminology remains canonical and consistent
- tests or checks cover the failure mode being addressed
- no transaction is held across external await points
- no float money types are introduced
- idempotency and retry behavior are explicit
- coordination-data retention is considered
- README / docs are updated if the developer workflow changed

Before declaring work done, the agent must self-audit against:

- the pinned Constitution document listed in `docs/foundation_lock.md`, especially Section 12 `Non-Goals`
- the pinned Constitution document listed in `docs/foundation_lock.md`, especially Section 13 `Canonical One-Liners`
- the pinned foundation documents listed in `docs/foundation_lock.md`

---

## 10. Practical note for agents

Do not be impressed by “what is easy to code”.
Be loyal to what the system is trying to protect.

In MUSUBI, code quality is not just syntax correctness.
It is the preservation of:

- dignity
- consent
- accountable reality
- append-only truth
- calm UX over extractive engagement

When in doubt, choose the design that keeps the product calmer, the truth source stricter, and the future migration path safer.
