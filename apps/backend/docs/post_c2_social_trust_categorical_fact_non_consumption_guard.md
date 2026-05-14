# Post-C2 Social Trust Categorical Fact Non-Consumption Guard

Status: implementation note for the narrow post-C2 non-consumption guard slice.

Design source:
- `docs/foundation_lock.md`
- upstream `docs/readiness/post_c2_implementation_handoff_evidence_package.md`
- upstream `docs/readiness/post_c2_non_consumption_guard_handoff_gate_decision.md`

This note describes only backend-local guard behavior.
It is not a new source-fact design, mutation-fact design, persistence design, migration design, public API design, projection refresh design, display design, scoring design, Relationship Depth design, or implementation handoff for another slice.

## Scope

This slice guards only already accepted C2 categorical Social Trust facts:

- accepted C2 bounded Promise reliability source fact labels
- accepted C2 categorical Social Trust mutation fact labels

The implementation lives in:

- `musubi_social_trust_domain` for the pure non-consumption decision contract
- deterministic social-trust-domain contract tests

It does not add database records, migrations, handlers, projection refresh, public API routes, mobile UI, runtime orchestration, new crates, or new dependencies.

## Allowed Internal Use

Accepted C2 categorical facts may remain internal writer fact references.

They may not become:

- numeric Social Trust score
- Social Trust score delta
- Social Trust weight
- Social Trust rank
- Social Trust display level
- public Social Trust display
- recovery ceiling
- discovery priority
- recommendation boost
- contact unlock
- room transition
- settlement progression
- Promise runtime outcome
- proof runtime outcome
- Relationship Depth fact
- projection refresh trigger
- public API response
- mobile UI state

## Non-Authority

Projection, analytics, observability, model output, client state, frontend state, payment, Support, popularity, engagement, recommendation, discovery, proof callback alone, vendor callback alone, operator notes, support tickets, issue comments, and implementation convenience remain non-authority.

Social Trust and Relationship Depth remain distinct.
Accepted C2 categorical Social Trust facts do not deepen Relationship Depth.

Everything outside this non-consumption guard remains blocked until a later accepted foundation decision narrows it.
