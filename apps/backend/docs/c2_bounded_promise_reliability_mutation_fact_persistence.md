# C2 Bounded Promise Reliability Mutation Fact Persistence

Status: implementation note for the narrow C2 persistence slice.

Design source:
- `docs/foundation_lock.md`
- upstream `docs/readiness/c2_bounded_promise_reliability_mutation_fact_gate.md`
- upstream `docs/readiness/c2_bounded_promise_reliability_implementation_handoff_gate.md`

This note describes only backend-local categorical persistence.
It is not a Promise runtime design, proof runtime design, public API design, projection refresh design, display design, scoring design, or implementation handoff for another slice.

## Scope

This slice persists only:
- accepted C2 bounded Promise reliability source fact references
- accepted C2 categorical Social Trust mutation facts
- no-effect valid excused exits
- forward correction, freeze, narrowing, and recovery categorical facts
- durable idempotency and deterministic replay checks
- minimized references needed to explain why persistence was allowed

The implementation lives in:
- `musubi_social_trust_domain` for pure source / mutation decision contracts
- `musubi_backend::services::social_trust_mutation` for backend-local persistence
- `social_trust.categorical_source_references`
- `social_trust.categorical_mutation_facts`

## Accepted Facts

Accepted source facts:
- `promise_reliability_outcome.completed_as_agreed`
- `promise_reliability_outcome.completed_after_governed_review`
- `promise_reliability_outcome.valid_excused_exit`
- `promise_reliability_outcome.source_fact_corrected`
- `promise_reliability_outcome.review_required_boundary_intersection`
- `promise_reliability_outcome.source_scope_limited_after_review`
- `promise_reliability_outcome.freeze_or_narrowing_reversed_after_review`

Accepted mutation facts:
- `social_trust_mutation.bounded_promise_reliability_positive`
- `social_trust_mutation.no_effect_valid_excused_exit`
- `social_trust_mutation.bounded_promise_reliability_correction`
- `social_trust_mutation.bounded_promise_reliability_freeze`
- `social_trust_mutation.bounded_promise_reliability_narrowing`
- `social_trust_mutation.bounded_promise_reliability_recovery`

All accepted facts remain categorical internal writer facts.
They do not carry numeric amount, score delta, point value, rank effect, display tier, discovery priority, recommendation boost, room transition, settlement progression, contact unlock, or public API effect.

## Persistence Rules

The service rejects before persistence when:
- the source fact is rejected or unknown
- the requested mutation fact is unknown
- the source-to-mutation mapping does not match the accepted gate
- authority posture is projection-only
- Consent, block / Withdrawal, Age Assurance, Legal Hold, Critical Harm, account lifecycle, anti-abuse, appeal, correction, fraud, collusion, scam, or safety posture remains unresolved for a positive contribution
- required writer source, Promise, Promise terms, Consent, block / Withdrawal, Age Assurance, Legal Hold, Critical Harm, account lifecycle, anti-abuse, safety, evidence-level, reason, retention, audit, or idempotency references are missing
- the subject is not an active Ordinary Account for a new fact

The exact `promise_reliability_outcome.review_required_boundary_intersection` source may persist only the categorical freeze fact.
That freeze is not a negative score, public badge, punishment, projection refresh, or display surface.
The persisted source reference stores the exact boundary intersection label so replay and review can distinguish Consent, block / Withdrawal, Age Assurance, Legal Hold, Critical Harm, lifecycle, appeal / correction / safety, anti-abuse, collusion / scam / coercion, and sensitive-exposure cases.

Identical duplicate delivery replays the existing stored fact.
Duplicate delivery with changed meaning fails closed as an idempotency conflict.
Replay is checked before the active-account check so an already recorded fact can replay after a later suspension without creating a new fact.

## Non-Authority

This slice does not authorize:
- Social Trust score, weight, rank, public display, display level, recovery ceiling, public status, or romantic advantage
- projection refresh from Social Trust writer facts
- discovery or recommendation use
- Relationship Depth mutation
- room progression
- settlement progression
- Promise runtime behavior
- proof runtime behavior outside accepted source references
- public API routes
- mobile UI
- raw PII or raw evidence storage inside Social Trust writer truth
- operator notes, support tickets, issue comments, popularity, payment behavior, Support status, engagement telemetry, frontend state, client state, observability, projection, provider callback alone, or implementation convenience as Social Trust authority

Everything outside this categorical persistence envelope remains blocked until a later accepted foundation decision narrows it.
