# Post-C2 Master / Submaster Operator Seat Runtime Non-Authority Scope

Status: backend-local documentation note for the narrow post-C2 Master / Submaster operator-seat runtime non-authority slice.

Design source:
- `docs/foundation_lock.md`
- upstream `docs/adr/0037_master_submaster_operator_seat_writer_fact_boundary.md`
- upstream `docs/readiness/post_c2_master_submaster_operator_seat_runtime_implementation_handoff_gate_decision.md`

This note records only a non-authority boundary.
It is not runtime implementation, runtime test design, schema design, migration design, public API design, projection refresh design, lifecycle-worker design, Legal Hold design, evidence access design, external counsel design, court-facing workflow design, legal-process intake design, treasury design, compensation design, Social Trust design, Relationship Depth design, or implementation handoff for another slice.

## Scope

This docs-only slice records that ADR-0037 Master / Submaster authority remains forward-only PostgreSQL writer fact authority.

No active Master / Submaster writer fact path is implemented or authorized by this note.

No Acting Master runtime behavior is implemented or authorized by this note.

No backend code, tests, schema, migrations, public API behavior, projection refresh, runtime orchestration, retry worker, queue, outbox, inbox, lifecycle behavior, deletion behavior, pruning behavior, archive behavior, Legal Hold runtime behavior, evidence access runtime behavior, key lifecycle behavior, external counsel runtime behavior, court-facing workflow, legal-process intake, treasury behavior, or compensation behavior is added by this note.

## Operator Seat Boundary

Master and Submaster are internal backstage operator-seat terms.

A Master or Submaster seat is not:

- an Ordinary Account state
- a profile state
- an account display label
- a public rank
- a Steward status
- a Social Trust level
- a Relationship Depth level
- a discovery advantage
- a recommendation advantage
- a romantic availability signal
- a Promise participant authority
- a settlement authority
- a proof authority
- a token-holder rank
- a contribution leaderboard outcome
- a wallet balance outcome
- an operator note
- a support ticket
- an issue comment
- projection state
- observability state
- dashboard state
- frontend state
- client state
- implementation convenience

If a natural person occupying a Master or Submaster seat also has an Ordinary Account, that Ordinary Account remains ordinary participation only.
The Ordinary Account does not inherit backstage authority, special discovery treatment, special recommendation treatment, romantic advantage, Social Trust advantage, or Relationship Depth advantage.

## Existing Backend Surfaces

Existing backend surfaces are not ADR-0037 Master / Submaster runtime authority, including:

- `core.accounts.account_class`
- `core.operator_role_assignments`
- `dao.review_cases`
- `dao.evidence_bundles`
- `dao.evidence_access_grants`
- `dao.operator_decision_facts`
- `dao.appeal_cases`
- `projection.review_status_views`
- backend-local operator review docs
- backend-local Legal Hold non-authority docs

Those surfaces may remain useful for their existing accepted scopes.
They must not be treated as active Master / Submaster seat assignments, suspensions, revocations, succession facts, Acting Master activations, Legal Hold placement authority, evidence access authority, evidence access grants under ADR-0037, or evidence access audit facts under ADR-0037.

The existence of `dao.evidence_access_grants` in the current implementation does not satisfy ADR-0037 evidence access grant and audit requirements for Master / Submaster runtime authority.

## Preserved ADR-0037 Boundary

Future implementation must preserve that:

- Master is an operator seat
- Submaster is an operator seat
- Ordinary Account is never Master or Submaster authority
- Controlled Exceptional Account or explicit backstage role assignment is required for seat exercise
- there is one Day 1 Master seat
- there are three Day 1 Submaster seats
- Submaster succession priority is deterministic
- Acting Master activation is forward-only and auditable
- Legal Hold placement authority is not evidence access authority
- evidence access authority is not Legal Hold placement authority
- every concrete evidence access attempt must be audited when evidence access runtime is later authorized
- projection is not operator-seat authority
- contribution evidence is not direct authority
- Master / Submaster authority does not create Social Trust or Relationship Depth
- Master / Submaster authority does not bypass Consent, block, withdrawal, Age Assurance, launch posture, safety boundaries, Promise boundaries, settlement truth, Social Trust, or Relationship Depth

## Blocked Future Prerequisites

A future active Master / Submaster writer fact unit requires a separate accepted foundation gate before implementation.

The ADR-0037 required fact families remain blocked future prerequisites, including:

- operator-seat identity facts
- seat family facts for Master or Submaster
- Day 1 seat-count policy facts or accepted equivalent configuration
- Controlled Exceptional Account or backstage role-assignment link facts
- assigned actor reference facts where lawful
- assignment reason code facts
- assignment evidence reference facts
- capability-scope facts
- effective timestamp facts
- expiry, renewal, or review due facts
- suspension facts
- revocation facts
- replacement or succession facts
- Submaster succession-rank facts
- Acting Master activation and deactivation facts
- conflict, compromise, unavailability, or lawful-incapacity marker facts where relevant
- Legal Hold placement capability facts
- evidence access capability facts
- separate evidence access grant facts
- separate evidence access audit facts
- retention class facts
- audit facts

Those items are listed here only as blocked future prerequisites.
They are not schema names, table names, enum names, runtime APIs, migrations, tests, operator UI, or implementation instructions in this repository.

## Non-Authorities

This docs-only note does not authorize:

- runtime implementation
- runtime tests
- test-only work
- schema-only work
- DDL
- migrations
- backend runtime code
- backend README updates
- public API changes
- mobile UI
- projection refresh
- runtime orchestration
- retry workers
- queues
- outbox changes
- inbox changes
- lifecycle runtime behavior
- pruning
- archive
- deletion
- Legal Hold runtime behavior
- evidence access runtime behavior
- key lifecycle behavior
- external counsel runtime behavior
- court-facing workflow
- legal-process intake
- treasury behavior
- compensation behavior
- active Master / Submaster writer fact creation
- Acting Master runtime behavior
- active Legal Hold writer fact creation
- evidence access grant or audit runtime creation
- Social Trust source facts
- Social Trust mutation facts
- Relationship Depth facts
- discovery
- recommendation
- room behavior
- settlement behavior
- Promise runtime behavior
- proof runtime behavior
- Social Trust scoring
- public trust display
- broad runtime implementation

Everything outside this non-authority documentation boundary remains blocked until a later accepted foundation decision narrows it.
