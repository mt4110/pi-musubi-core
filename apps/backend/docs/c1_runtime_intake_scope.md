# C1 Runtime Intake Scope

Status:
Draft implementation-repo intake note.

Owner:
pi-musubi-core.

Scope:
Backend-local intake scope for the C1 trust/depth mutation registry and non-authority boundary.

Purpose:
Record what the backend may inspect next, what it must not treat as authority, and what remains blocked before any C1 runtime implementation PR.

---

## Current Gate State

The pinned foundation state is `musubi-foundation` commit `fcb5573b668b6875cf9c983770ad90f9de655e82`.

The accepted foundation C1 handoff records are upstream `mt4110/musubi-foundation` paths at the pinned foundation commit, not local `pi-musubi-core` files:

- `docs/readiness/c1_runtime_behavior_boundary.md`
- `docs/readiness/c1_runtime_handoff_evidence_package.md`
- `docs/readiness/c1_runtime_handoff_gate_decision.md`

The current C1 runtime implementation gate result remains NO-GO.

This note does not authorize runtime implementation.
This note does not authorize DDL.
This note does not authorize migrations.
This note does not authorize runtime tests.
This note does not authorize API changes.
This note does not authorize app code changes.

## Local Surfaces Already Present

The backend already has projection-owned trust posture surfaces:

- `projection.trust_snapshots`
- `projection.realm_trust_snapshots`
- `GET /api/projection/trust-snapshots/{account_id}`
- `GET /api/projection/realm-trust-snapshots/{realm_id}/{account_id}`

These surfaces are read models only.
They are not Social Trust or Relationship Depth writer truth.
They must not be reused as C1 mutation authority, repair authority, ranking authority, recommendation authority, or consent authority.

The backend also already has writer-owned boundaries that future C1 work may need to inspect:

- `dao` append-only coordination facts
- operator review and appeal facts
- room progression writer facts
- proof input facts
- settlement and Promise writer facts
- outbox / inbox coordination records
- projection rebuild metadata

This note does not select any of those as C1 implementation scope.

## Local Intake Decision

The next safe backend step is not to mutate trust/depth.

The next safe backend step is to keep C1 scoped as an intake and non-authority planning problem until a later PR explicitly names:

- the exact writer-owned fact families C1 may consume;
- the exact writer-owned fact family C1 may create, if any;
- the durable idempotency boundary for a proposed mutation attempt;
- the retention class for `CandidateForWriterPersistence` and rejected attempts;
- the outbox / inbox behavior for cross-process effects;
- the broken-path tests that prove forbidden sources fail closed;
- the projection refresh posture after writer facts exist.

The next local scope note for that narrowing is:

- `c1_social_trust_writer_fact_scope.md`

## Forbidden Shortcut

A future implementation must not:

- calculate trust/depth from `projection.trust_snapshots`;
- calculate trust/depth from `projection.realm_trust_snapshots`;
- treat projection lag, observability, analytics, client state, frontend state, payment, Support, popularity, reply speed, dwell time, recommendation state, or discovery state as mutation authority;
- add a public trust score, leaderboard, or ranking from C1;
- bypass consent, block, withdrawal, age/safety, or review gates because a trust/depth posture exists;
- hold an authoritative transaction open across provider, model, analytics, notification, search, or other network I/O;
- create broad C1 tables, APIs, workers, or migration scope by convenience.

## Candidate Future PR Shape

The first future C1 implementation PR, if a later gate explicitly permits it, should be narrow enough to review as one behavioral contract.

Recommended first contract:

- `Social Trust proposed mutation attempt intake / no-authority decision contract`

See `c1_social_trust_intake_contract_scope.md`.

The follow-up writer-fact planning boundary is:

- `c1_social_trust_writer_fact_scope.md`

Recommended first candidate shape:

- no public API;
- no mobile UI;
- no discovery or recommendation changes;
- no projection-based authority;
- no payment or Support influence;
- one writer-owned C1 intake path or one deterministic no-authority guard, not both;
- deterministic tests focused on duplicate delivery, missing source references, forbidden source categories, and projection non-authority.

This candidate shape is not authorization.
It is the smallest shape that appears compatible with the pinned foundation constraints.

## Stop Conditions

Stop before implementation if any of the following remain unclear:

- whether a source is an accepted writer fact or a forbidden authority source;
- whether the fact is PII-bearing, append-only, coordination-only, or projection-only;
- whether replay should preserve an existing result or reject payload drift;
- whether the future work needs a new durable domain noun not present in the pinned foundation docs;
- whether a projection refresh is being confused with writer truth;
- whether a test would require inventing exact trust/depth weights or mutation magnitude.

## Bottom Line

The backend is now aligned enough to plan C1 locally.

It is not yet authorized to implement C1 runtime behavior.

The next useful PR should either keep narrowing C1 into exact backend implementation scope or, if separately authorized, implement one small writer-owned C1 intake / no-authority contract without touching projection as authority.
