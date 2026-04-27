# Projection Read Models

This note documents the derived read-side boundary added for Issue #22.

The important rule is simple: projection data is rebuildable convenience state.
It is never writer-owned business truth and must not be used to decide settlement progression, reward progression, safety freezes, or provider-side actions.

## Source Boundary

Projection rows are derived only from PostgreSQL writer-owned facts established before this issue:

- `dao.promise_intents`
- `dao.settlement_cases`
- `dao.settlement_submissions`
- `dao.settlement_observations`
- `core.payment_receipts`
- `ledger.journal_entries`
- `ledger.account_postings`

Raw provider callbacks remain intake evidence and do not leak into user-facing read models.
Proof persistence is still unavailable as writer truth, so proof-derived fields are exposed as `proof_status = unavailable` with `proof_signal_count = 0`.

## Tables

Issue #22 adds or expands these projection-owned tables:

- `projection.promise_views`
- `projection.settlement_views`
- `projection.trust_snapshots`
- `projection.realm_trust_snapshots`
- `projection.projection_meta`

Every row carries provenance fields:

- `source_watermark_at`
- `source_fact_count`
- `freshness_checked_at`
- `projection_lag_ms`
- `last_projected_at`
- `rebuild_generation`, when created by a full rebuild

## HTTP Read API

All user-facing read endpoints require a bearer token.

Existing baseline parity remains unchanged:

- `GET /api/projection/settlement-views/{settlement_case_id}`

Issue #22 adds:

- `GET /api/projection/promise-views/{promise_intent_id}`
- `GET /api/projection/settlement-views/{settlement_case_id}/expanded`
- `GET /api/projection/trust-snapshots/{account_id}`
- `GET /api/projection/realm-trust-snapshots/{realm_id}/{account_id}`

Promise and settlement projections are visible only to the Promise participants.
Current trust projection visibility is self-scoped.
Realm trust snapshots include `realm_id`; global trust snapshots deliberately do not expose realm-local details.

## Rebuild API

The internal rebuild path is:

- `POST /api/internal/projection/rebuild`

It is mounted under the same internal/debug gate as the orchestration drain.
In release builds, internal routes also require `Authorization: Bearer $MUSUBI_INTERNAL_API_TOKEN`.
The rebuild clears projection rows only, then regenerates them from writer-owned facts in one writer transaction.
It also updates `projection.projection_meta` with row counts, source fact counts, watermarks, lag, and the rebuild generation.

## Trust Boundary

Trust output is intentionally bounded:

- no single global trust score
- no ranking
- no leaderboard
- no popularity metric
- no recommendation-engine behavior

The trust projection exposes a calm posture plus reason codes such as:

- `promise_participation_observed`
- `deposit_backed_promise_funded`
- `manual_review_bucket_nonzero`
- `proof_unavailable`
- `realm_scoped`, only on realm-local snapshots

These reason codes are derived from bounded writer facts.
They are not proof of human worth and do not override consent.
