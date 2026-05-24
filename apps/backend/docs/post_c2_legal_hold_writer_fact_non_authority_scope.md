# Post-C2 Legal Hold Writer Fact Non-Authority Scope

Status: backend-local documentation note for the narrow post-C2 Legal Hold writer fact downstream scope slice.

Design source:
- `docs/foundation_lock.md`
- upstream `docs/adr/0011_legal_hold_evidence_preservation_boundary.md`
- upstream `docs/readiness/post_c2_legal_hold_writer_fact_downstream_scope_evidence_package.md`
- upstream `docs/readiness/post_c2_legal_hold_writer_fact_downstream_scope_handoff_gate_decision.md`

This note records only a non-authority boundary.
It is not runtime implementation, runtime test design, schema design, migration design, public API design, projection refresh design, lifecycle-worker design, evidence access design, deletion design, pruning design, archive design, key lifecycle design, Social Trust design, Relationship Depth design, or implementation handoff for another slice.

## Scope

This docs-only slice records that ADR-0011 Legal Hold remains a scoped PostgreSQL writer state for preservation authority.

`mt4110/pi-musubi-core#64` remains open and not implementation-ready.

No active Legal Hold writer fact path is implemented or authorized by this note.

No backend code, tests, schema, migrations, public API behavior, projection refresh, runtime orchestration, retry worker, queue, outbox, inbox, lifecycle behavior, deletion behavior, pruning behavior, archive behavior, evidence access behavior, or key lifecycle behavior is added by this note.

## Legal Hold Boundary

ADR-0011 Legal Hold is preservation authority only.
It is not evidence access authority.

A Legal Hold is not:

- an operator note
- a support ticket
- an issue comment
- a projection flag
- an observability event
- a dashboard label
- a provider callback
- proof evidence by itself
- Device Attestation by itself
- Proximity Proof by itself
- ZK proof by itself
- a client claim
- frontend state
- routing state
- implementation convenience
- payment hold language
- settlement hold language
- manual operator hold language
- safety review label
- Social Trust boundary-intersection reference

Evidence access remains a separate writer-owned grant, role check, scope check, and audit fact problem.
This note does not implement or authorize evidence access.

## Accepted Hold Classes

The accepted ADR-0011 hold class identifiers are:

- `legal_process`
- `critical_harm_or_safety`
- `youth_safety`
- `settlement_or_tax`
- `abuse_or_integrity_investigation`
- `appeal_or_review_preservation`

This note does not add hold classes.
It does not add jurisdiction-specific subclasses.
Operational H0 / H1 / H2 / H3 / H4 descriptions in foundation detail documents remain review language only unless a later accepted foundation extension maps them into ADR-0011 identifiers or changes the taxonomy.

## Existing Backend Labels

Existing hold-like labels, review labels, settlement hold language, Social Trust boundary references, projection state, observability, provider callbacks, proof evidence, support tickets, issue comments, operator notes, client state, and frontend state must not be treated as ADR-0011 Legal Hold writer facts.

Existing implementation references to Legal Hold intersections are unresolved boundary references.
They do not create active Legal Hold facts, evidence access grants, lifecycle-worker authority, deletion authority, pruning authority, archive authority, Subject Tombstone behavior, key lifecycle behavior, Social Trust source facts, Social Trust mutation facts, Relationship Depth facts, settlement truth, Promise runtime truth, proof runtime truth, discovery input, recommendation input, public display, or romantic availability.

## Blocked Future Prerequisites

A future active Legal Hold writer fact unit requires a separate accepted foundation gate before implementation.

The blocked active-hold minimum facts remain:

- `hold_id`
- accepted hold class
- reason code
- structured scope anchor
- scope facts
- evidence class or record family
- placed-by role
- placed-by actor reference where lawful
- created timestamp
- review due timestamp
- renewal or expiry path
- lift conditions
- linked case, incident, legal process, settlement case, or review object where relevant

Those items are listed here only as blocked future prerequisites.
They are not schema names, table names, enum names, runtime APIs, migrations, tests, or implementation instructions in this repository.

## Non-Authorities

This docs-only note does not authorize:

- runtime implementation
- runtime tests
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
- key lifecycle behavior
- evidence access runtime behavior
- active Legal Hold writer fact creation
- invalid Legal Hold rejection persistence
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
