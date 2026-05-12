# C1 Social Trust Writer Fact Scope

Status:
Draft implementation-repo scope note.

Owner:
pi-musubi-core.

Scope:
Docs-only boundary for the next Social Trust writer-fact persistence planning step after the pure intake contract.

Purpose:
Name the writer-owned fact families, idempotency boundary, retention posture, outbox / inbox posture, broken-path tests, and projection refresh posture that must be accepted before any C1 Social Trust persistence PR.

---

## Current Authorization State

This document does not authorize runtime implementation.
This document does not authorize DDL.
This document does not authorize migrations.
This document does not authorize PostgreSQL writer tables.
This document does not authorize HTTP routes or Axum handlers.
This document does not authorize mobile UI.
This document does not authorize projection writes.
This document does not authorize Social Trust scoring, weights, rank, display level, narrowing, freeze, recovery, or public presentation.

The C1 runtime implementation gate remains NO-GO.

## Decision

The next safe Social Trust step is to define a future writer boundary for `Social Trust proposed mutation attempt` intake facts only.

A later persistence PR, if separately authorized, may at most persist the intake attempt, its deterministic intake decision, durable idempotency posture, retention posture, and audit posture.

That future step must still not persist an actual Social Trust mutation, score, weight, rank, display level, narrowing fact, freeze fact, or recovery fact.

## Future Writer-Owned Fact Families C1 May Consume Later

A future C1 persistence boundary may consume only writer-owned facts that already exist under accepted foundation authority and are read from the writer.

Allowed consumption families:

- Trust / depth mutation registry entries from ADR-0019.
- Social Trust source category facts from ADR-0019 and ADR-0020.
- Writer source-reference facts that point to an accepted source family without copying raw evidence.
- Mutation reason facts or reason-code facts.
- Bounded evidence-level facts or evidence posture facts.
- Reviewability facts and scoped operator role facts where review is required.
- Durable idempotency facts for mutation attempts.
- Retention class facts from ADR-0012.
- Consent, block, withdrawal, Age Assurance, safety, deletion, tombstone, Legal Hold, and account-lifecycle references only as upper-boundary or lifecycle checks where applicable.
- Audit facts for intake, rejection, replay, review, and source-reference access.

No positive product source family is selected by this document.
Promise completion, settlement outcome, Meeting, Reflection, room transition, proof evidence, or proof eligibility may become Social Trust source references only after a later accepted boundary explicitly permits that source category and records the effect through writer-owned Social Trust facts.
This document does not make any of those families eligible, implementation-ready, or selected for C1 persistence.

## Writer-Owned Fact Families C1 May Create Later

A later C1 persistence PR, if separately authorized, may create only these writer-owned intake families:

- Social Trust proposed mutation attempt facts.
- Social Trust intake decision facts, including rejected decisions and `CandidateForWriterPersistence` decisions.
- Durable idempotency / replay facts for proposed mutation attempts.
- Minimal reason, retention, reviewability, evidence-posture, and audit facts needed to explain the intake decision.

This is a maximum envelope for a later accepted persistence PR, not authorization from this document.

That later PR must not create:

- Social Trust subject facts.
- Social Trust mutation facts.
- Social Trust mutation direction facts.
- Social Trust narrowing, freeze, or recovery facts.
- Public display facts.
- Projection rows.
- Discovery, recommendation, room progression, proof, settlement, Promise, or Relationship Depth facts.

## Durable Idempotency Boundary

The proposed mutation attempt boundary must be database-enforced in a later persistence PR.
In-memory dedupe is not acceptable.

The durable dedupe identity must be scoped tightly enough to prevent accidental replay across subjects or source facts.
The future boundary should include, at minimum:

- subject reference from accepted account-continuity writer facts, not a new Social Trust subject fact.
- source category.
- writer source reference.
- reason fact reference.
- policy or schema version.
- caller-provided or system-derived attempt idempotency key.

Duplicate delivery with the same dedupe identity and identical request meaning must preserve the existing recorded decision.
Duplicate delivery with the same dedupe identity but payload drift must fail closed as an idempotency conflict or review-required contradiction.

The dedupe identity must not include raw PII, raw evidence payloads, payment amount, popularity, engagement metrics, projection state, or operator notes.

## Retention Class Posture

`CandidateForWriterPersistence` attempts and rejected attempts are material record families and must be retention-classified before production use.
This document does not invent concrete retention durations.
Both attempt outcomes must use an accepted Retention Class Registry entry for the ADR-0012 category `Social Trust evidence or future Social Trust writer facts` before production use.

`CandidateForWriterPersistence` attempts:

- are not Social Trust mutation truth;
- must be retained as intake / audit / replay facts under that accepted lifecycle class;
- must keep minimized source references and payload hashes rather than raw evidence payloads;
- must preserve enough audit metadata for replay, appeal, repair, and forward correction where later accepted policy permits.

Rejected attempts:

- must be retained under the same accepted lifecycle category unless a later accepted retention policy narrows rejected-attempt handling;
- must retain only the minimum reason-coded rejection, dedupe posture, source category, source reference posture, and audit metadata needed for replay safety and review;
- must not retain forbidden raw source payloads by convenience;
- must not become a shadow archive of PII, projection rows, observability payloads, payment behavior, or operator notes;
- must follow Legal Hold, deletion, tombstone, and key-lifecycle constraints where applicable.

Outbox and inbox records related to C1, if any are added later, are coordination logs.
They must have bounded retention, quarantine, archive, or pruning rules from Day 1.

## Outbox / Inbox Posture

Pure intake decisioning does not require outbox or inbox behavior by itself.

A later persistence PR must add outbox / inbox only if the accepted scope crosses a process boundary, performs asynchronous review, schedules downstream work, or triggers any external side effect.

If outbox / inbox is introduced:

- the authoritative intake facts must commit before any delivery work;
- producer outbox cleanup must not become business truth;
- consumers that mutate writer truth must use durable inbox dedupe;
- messages must use schema versions, stable event identities, payload hashes, and pseudonymous references;
- workers must drop authoritative transactions before awaiting provider, queue, analytics, model, notification, or other network I/O;
- completed coordination rows must be pruned, archived, or compacted under ADR-0002 and ADR-0012.

Rejected attempts should not emit projection refresh work.
`CandidateForWriterPersistence` attempt facts should not emit projection refresh work.
Projection refresh becomes eligible only after a separate accepted boundary creates actual Social Trust writer facts whose derived posture needs to be refreshed.

## Projection Refresh Posture

Projection remains non-authoritative.

After future accepted Social Trust writer facts exist, a projection refresh may derive calm, self-scoped, participant-safe, or operator-scoped posture from those writer facts only where access policy permits.

Projection refresh must not:

- calculate Social Trust from projection rows;
- treat a proposed mutation attempt as a completed Social Trust mutation;
- treat projection freshness as writer truth;
- repair writer facts from projection state;
- rank, recommend, boost, shame, or publicly score people;
- bypass Consent, block, withdrawal, Age Assurance, Legal Hold, safety, or account-lifecycle controls.

Projection lag must fail safe.
If writer facts and projection disagree, writer facts win and projection is rebuilt or refreshed as disposable read-side state.

## Required Broken-Path Tests For A Later Persistence PR

A later persistence PR must include deterministic tests for these failure paths:

- forbidden source categories fail closed before any Social Trust mutation fact is written;
- unknown source categories fail closed;
- projection trust snapshot references cannot become Social Trust authority;
- payment amount and payment frequency cannot create or raise Social Trust;
- Support amount or Support status cannot create or raise Social Trust;
- recommendation state, discovery state, and discovery ranking cannot create or raise Social Trust;
- popularity, follower count, reply speed, dwell time, tenure, romantic desirability, engagement, and engagement telemetry cannot create or raise Social Trust;
- Relationship Depth and room projection cannot create or raise Social Trust;
- operator notes, support tickets, and issue comments cannot grant Social Trust;
- anti-abuse marker existence cannot directly raise or lower Social Trust;
- Age Assurance posture cannot raise Social Trust;
- proof callback alone and vendor callback alone cannot create Social Trust;
- Controlled Exceptional Account activity cannot create ordinary Social Trust;
- missing writer source reference fails closed;
- missing reason fact fails closed;
- missing durable idempotency posture fails closed;
- duplicate delivery is database-deduped;
- duplicate delivery with payload drift fails closed;
- missing retention posture fails closed;
- missing evidence or reviewability posture fails closed where required;
- rejected attempts do not emit projection refresh work;
- `CandidateForWriterPersistence` attempt facts do not create Social Trust score, weight, rank, display level, narrowing, freeze, or recovery facts.

## Stop Conditions

Stop before implementation if any of these are still unresolved:

- a positive Social Trust source family has not been accepted;
- the future work would require exact Social Trust weights or mutation magnitude;
- the future work would require new durable vocabulary outside the pinned foundation docs;
- an accepted Retention Class Registry entry is not named or cannot be mapped without inventing durations;
- idempotency cannot be enforced by durable writer facts or database constraints;
- projection refresh is being treated as writer truth;
- outbox / inbox cleanup is being treated as business completion truth;
- the implementation would touch Relationship Depth, proof eligibility, room progression, discovery, recommendation, settlement, Promise, HTTP routes, mobile UI, or public trust display.

## Bottom Line

C1 may be narrowed toward Social Trust intake persistence, but the only safe next writer boundary is proposed mutation attempt intake facts.

Actual Social Trust mutation facts still need a later, separate acceptance boundary.
