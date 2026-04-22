# Realm Bootstrap Surface

Design source: ISSUE-15-realm-bootstrap-and-admission.md

The GitHub issue number is intentionally not hardcoded.

## Purpose

ISSUE-15 adds the first bounded Realm bootstrap and admission baseline.

It exists to solve a narrow launch problem:
- fully manual Realm creation does not scale
- fully public self-serve Realm creation would become cheap Sybil issuance

The implemented shape is explicit and boring:
- authenticated Realm creation requests
- operator-reviewed Realm approval or rejection
- explicit sponsor records with bounded quota
- temporary bootstrap corridors with server-enforced caps and expiry
- server-derived admissions
- rebuildable participant-safe and operator-safe projections

This is not public realm issuance, not a referral loop, and not a growth hack.

## Writer-owned truth

Writer truth lives in `dao`:
- `dao.realm_requests`
- `dao.realms`
- `dao.realm_sponsor_records`
- `dao.bootstrap_corridors`
- `dao.realm_admissions`
- `dao.realm_review_triggers`

Important boundaries:
- requests are reviewable facts, not self-serve realm activation
- sponsor quota is enforced from writer-owned admission rows
- corridor expiry and corridor caps are enforced on the writer path
- restricted or suspended realms block new admissions on the writer path
- idempotency is durable for retryable request, approval, sponsor-record, and admission writes

Projection rows are never used to decide whether a Realm can admit someone.

## Projection surface

Derived read models live in `projection`:
- `projection.realm_bootstrap_views`
- `projection.realm_admission_views`
- `projection.realm_review_summaries`

Participant-safe summary:
- realm display identity
- safe realm status
- admission posture such as `open`, `limited`, or `review_required`
- current viewer admission status when present
- safe sponsor/steward display state when intentionally public

Participant-safe summary does not expose:
- operator IDs
- raw review trigger context
- source fact IDs
- source fact counts
- moderation internals
- raw evidence
- operator notes

Operator/steward summary may expose:
- current realm status
- corridor status and remaining time
- sponsor-backed usage counts
- open review trigger counts
- open review case counts
- redacted reason codes
- freshness metadata

Operator/steward summary still does not become writer authority.

## HTTP surface

Participant-facing:
- `POST /api/realms/requests`
- `GET /api/realms/requests/{realm_request_id}`
- `GET /api/projection/realms/{realm_id}/bootstrap-summary`

Internal/debug-gated:
- `GET /api/internal/operator/realms/requests`
  - accepts `limit`, `before_created_at`, and `before_realm_request_id`
  - the store enforces a bounded page size; callers page with the last row's `created_at` and `realm_request_id`
- `GET /api/internal/operator/realms/requests/{realm_request_id}`
  - includes request-scoped open review triggers with redacted reason codes only; raw trigger context stays writer-owned
- `POST /api/internal/operator/realms/requests/{realm_request_id}/approve`
- `POST /api/internal/operator/realms/requests/{realm_request_id}/reject`
- `POST /api/internal/realms/{realm_id}/sponsor-records`
- `POST /api/internal/realms/{realm_id}/admissions`
- `GET /api/internal/operator/realms/{realm_id}/review-summary`
- `POST /api/internal/projection/realms/rebuild`

The internal operator routes reuse the existing internal/debug gate and `x-musubi-operator-id` role checks. ISSUE-15 does not add a separate launch framework.

Projection rebuilds use set-based `INSERT ... SELECT` refreshes for bootstrap views, admission views, and review summaries. Rebuild output stays derived display state and must not be used for writer-owned admission decisions.

## Policy posture

The bootstrap corridor is intentionally bounded:
- sponsor-backed admissions are quota-bounded
- rate-limited or revoked sponsors do not auto-admit new members
- corridor benefits stop when the corridor expires or is disabled
- quota/cap pressure opens internal review triggers instead of silently admitting

The participant-facing copy must stay calm:
- no guaranteed admission language
- no urgency loop
- no ranking or boost framing
- no DM unlock or paid romantic advantage framing

## Integration boundaries

ISSUE-15 reuses existing work and does not replace it:
- ISSUE-12 operator review remains the review/evidence workflow
- ISSUE-13 room progression remains the room progression writer/projection baseline
- ISSUE-14 Promise UI remains out of scope here

This implementation also does not add:
- Promise proof persistence
- Promise completion writer truth
- public realm discovery
- federation
- governance
- referral growth loops
- ranking or recommendation boosts
