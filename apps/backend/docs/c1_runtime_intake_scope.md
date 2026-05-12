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

The pinned foundation state is `musubi-foundation` commit `f576bcd826b7070f573ef8276c68aff5d0ae864e`.

The accepted foundation C1 handoff records are upstream `mt4110/musubi-foundation` paths at the pinned foundation commit, not local `pi-musubi-core` files:

- `docs/readiness/c1_runtime_gate_invocation_guard.md`
- `docs/readiness/c1_runtime_behavior_boundary.md`
- `docs/readiness/c1_runtime_handoff_evidence_package.md`
- `docs/readiness/c1_runtime_handoff_gate_decision.md`
- `docs/readiness/c1_social_trust_intake_handoff_gate_decision.md`

The broad C1 runtime implementation gate result remains NO-GO.
The current C1 Social Trust intake handoff result is `NARROW GO FOR ONE LATER IMPLEMENTATION-REPO PR`.

The authorized implementation-repo slice is Social Trust proposed mutation attempt intake persistence only.
This authorization is limited to additive backend persistence, deterministic tests, and documentation that keeps the boundary explicit.

This note still does not authorize broad runtime implementation.
This note does not authorize API changes.
This note does not authorize HTTP handlers, public routes, mobile UI, projection refresh work, or app-state exposure.

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

The next safe backend step is to persist only the C1 Social Trust intake / non-authority decision boundary authorized by the pinned handoff gate:

- proposed mutation attempt intake records;
- rejected and `CandidateForWriterPersistence` intake decisions;
- durable database-enforced idempotency / replay posture;
- minimized reason, evidence, reviewability, retention, and audit metadata;
- deterministic tests for forbidden sources, missing posture, duplicate replay, payload drift, and projection non-authority.

This is intake persistence only.
It is not Social Trust mutation persistence.

## Forbidden Shortcut

A future implementation must not:

- calculate trust/depth from `projection.trust_snapshots`;
- calculate trust/depth from `projection.realm_trust_snapshots`;
- treat projection lag, observability, analytics, client state, frontend state, payment, Support, popularity, reply speed, dwell time, recommendation state, or discovery state as mutation authority;
- add a public trust score, leaderboard, or ranking from C1;
- bypass consent, block, withdrawal, age/safety, or review gates because a trust/depth posture exists;
- hold an authoritative transaction open across provider, model, analytics, notification, search, or other network I/O;
- create broad C1 tables, APIs, workers, or migration scope by convenience.

## Authorized Narrow PR Shape

The first C1 implementation PR is narrow enough to review as one behavioral contract plus one persistence boundary.

Selected contract:

- `Social Trust proposed mutation attempt intake / no-authority decision contract`

See `c1_social_trust_intake_contract_scope.md`.

The follow-up writer-fact planning boundary is:

- `c1_social_trust_writer_fact_scope.md`

Authorized first candidate shape:

- no public API;
- no mobile UI;
- no discovery or recommendation changes;
- no projection-based authority;
- no payment or Support influence;
- one writer-owned C1 intake persistence path;
- deterministic tests focused on duplicate delivery, missing source references, forbidden source categories, and projection non-authority.

## Stop Conditions

Stop before implementation if any of the following remain unclear:

- whether a source is an accepted writer fact or a forbidden authority source;
- whether the fact is PII-bearing, append-only, coordination-only, or projection-only;
- whether replay should preserve an existing result or reject payload drift;
- whether the future work needs a new durable domain noun not present in the pinned foundation docs;
- whether a projection refresh is being confused with writer truth;
- whether a test would require inventing exact trust/depth weights or mutation magnitude.

## Bottom Line

The backend is now aligned enough to implement one narrow C1 Social Trust intake persistence PR.

The authorized implementation must remain one small writer-owned C1 intake / no-authority persistence boundary without touching projection as authority.
