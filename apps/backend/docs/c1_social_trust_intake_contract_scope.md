# C1 Social Trust Intake Contract Scope

Status:
Draft implementation-repo scope note.

Owner:
pi-musubi-core.

Scope:
Exact first C1 backend implementation candidate for Social Trust intake / no-authority decision behavior.

Purpose:
Select one narrow writer-contract path for future implementation without authorizing runtime implementation in this PR.

---

## Decision

The recommended first C1 implementation contract is:

`Social Trust proposed mutation attempt intake / no-authority decision contract`.

This selects issue `mt4110/pi-musubi-core#58` as the first C1 implementation candidate.

This does not select Relationship Depth.
This does not select proof-to-trust/depth.
This does not select discovery or recommendation behavior.
This does not select room progression / Relationship Depth ambiguity work.

## Current Authorization State

This document does not authorize runtime implementation.
This document does not authorize DDL.
This document does not authorize migrations.
This document does not authorize runtime tests.
This document does not authorize API changes.
This document does not authorize mobile UI changes.
This document does not authorize app-service wiring.

The C1 runtime implementation gate result remains NO-GO until a later PR explicitly authorizes one narrow implementation step.

## Why Social Trust First

Social Trust is the best first C1 implementation candidate because it can be constrained to conduct-reliability intake and forbidden-source rejection without touching Relationship Depth mutuality, room progression semantics, discovery ranking, recommendation ranking, public display, or proof eligibility.

Relationship Depth should come later because it depends on consent, mutuality, and room/progression boundaries that are easier to contaminate accidentally.

Proof-to-trust/depth should come later because proof evidence must remain input evidence until accepted eligibility and source-reference rules are implemented.

Discovery / recommendation should come later because those systems must consume only read-only posture and must not create trust/depth authority.

## Future Implementation Shape

If a later PR authorizes implementation, the first implementation should be a pure backend domain contract.

Recommended future write scope:

- add a new backend domain crate for trust/depth contract types and decision logic;
- add the new crate to the backend workspace;
- add unit tests inside that new crate.

Forbidden future write scope for the first implementation:

- no database migrations;
- no PostgreSQL writer tables;
- no HTTP routes;
- no Axum handlers;
- no mobile UI;
- no projection writes;
- no projection reads as authority;
- no discovery or recommendation changes;
- no settlement, proof, room progression, operator review, or launch posture behavior changes;
- no public Social Trust score.

This scope intentionally avoids `musubi_core_domain` because that crate currently owns neutral account-adjacent identifiers only.
Social Trust must not pollute the core account boundary by convenience.

The next docs-only boundary after this pure contract is `c1_social_trust_writer_fact_scope.md`.
That note names the future writer-owned fact families, idempotency posture, retention posture, outbox / inbox posture, broken-path tests, and projection refresh posture without authorizing DDL, migrations, API wiring, projection writes, or Social Trust mutation facts.

## Contract Behavior To Implement Later

The future pure domain contract should decide whether a proposed Social Trust mutation attempt is:

- rejected because the source category is forbidden;
- rejected because required source reference facts are missing;
- rejected because idempotency posture is missing;
- rejected because evidence / reviewability / retention posture is missing where required;
- classified only as `CandidateForWriterPersistence` for later writer-owned persistence, not as an immediate trust mutation.

The contract must fail closed for unknown source categories.

The contract must not compute Social Trust weight, score, rank, display level, or recovery ceiling.

## Forbidden Source Categories

The first implementation must reject these as Social Trust authority:

- projection state;
- observability state;
- client state;
- frontend state;
- payment amount;
- payment frequency;
- Support amount or Support status;
- popularity;
- follower count;
- reply speed;
- dwell time;
- engagement telemetry;
- recommendation state;
- discovery ranking;
- Relationship Depth;
- room projection;
- operator notes;
- support tickets;
- issue comments;
- anti-abuse marker existence;
- Age Assurance posture;
- proof callback alone;
- vendor callback alone;
- Controlled Exceptional Account activity;
- implementation convenience.

These categories are derived from pinned upstream ADR-0019 and ADR-0020.

## Positive Source Handling

The first implementation should not create a broad allowlist of Social Trust sources.

It may model only the minimum source posture needed to prove that:

- forbidden categories fail closed;
- unknown categories fail closed;
- missing writer source references fail closed;
- projection-only posture fails closed.

Any positive source category that would actually raise, lower, freeze, narrow, or recover Social Trust must wait for a later writer-fact persistence PR.

## Test Focus For The Later Implementation

The future implementation tests should prove:

- projection cannot mutate Social Trust;
- payment amount cannot mutate Social Trust;
- recommendation state cannot mutate Social Trust;
- Controlled Exceptional Account activity cannot create ordinary Social Trust;
- operator notes cannot grant Social Trust;
- Age Assurance posture cannot raise Social Trust;
- anti-abuse marker existence cannot directly lower Social Trust;
- unknown source category fails closed;
- missing source reference fails closed;
- missing idempotency posture fails closed.

Those tests should be pure unit tests in the new domain crate.

They should not require PostgreSQL, Redis, HTTP, migrations, provider calls, or mobile UI.

## Relationship To Open Issues

This scope targets:

- `mt4110/pi-musubi-core#58` - ADR-0010 blocker: Social Trust writer facts are not implementation-ready.

This scope does not address:

- `mt4110/pi-musubi-core#59` - Relationship Depth writer facts are not implementation-ready;
- `mt4110/pi-musubi-core#60` - proof-to-trust-depth path is not implementation-ready;
- `mt4110/pi-musubi-core#61` - discovery recommendation constraints are not implementation-ready;
- `mt4110/pi-musubi-core#62` - room_progression.relationship must not be treated as Relationship Depth.

## Bottom Line

The first C1 implementation should be Social Trust intake / no-authority decision logic only.

It should prove that forbidden and projection-only sources cannot become Social Trust authority before the project attempts any persistent Social Trust writer facts.
