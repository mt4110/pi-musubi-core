# Operator Review Workflow

Design source: ISSUE-12-operator-review-appeal-evidence.md

The GitHub issue number is intentionally not hardcoded. The design source number is a product/design reference, not a repository issue identifier.

ISSUE-12 adds the baseline operator review, appeal, and evidence workflow. It is a human review foundation for later safety and product surfaces, including ISSUE-13 room progression and ISSUE-14 Promise UI fallback states. Those later surfaces are consumers of this workflow and are not implemented here.

## Boundary

Operator decisions are append-only facts in `dao.operator_decision_facts`.

An operator decision must not overwrite the original writer-owned truth row that opened the review. Review cases reference source facts through `source_fact_kind`, `source_fact_id`, and an optional source snapshot, while the source tables remain unchanged. User-facing state is derived into `projection.review_status_views`.

This keeps the workflow auditable:
- `dao.review_cases` tracks the case envelope and source linkage.
- `dao.evidence_bundles` stores evidence summaries and raw evidence locators separately.
- `dao.evidence_access_grants` records scoped, expiring evidence access approvals.
- `dao.operator_decision_facts` records decisions as append-only facts.
- `dao.appeal_cases` links appeals back to the original review case or decision fact.
- `projection.review_status_views` exposes bounded, calm user-facing status and reason codes.

## Evidence Access

Evidence access is separate from case visibility. A case may exist without granting an operator raw evidence access.

Access scopes are intentionally boring and bounded:
- `summary_only`
- `redacted_raw`
- `full_raw`

Granting access requires an approver role. The grantee must also have a role allowed for the requested scope. Expired or revoked grants are not a reusable permission source.

Raw evidence locators stay out of handler responses and user-facing projections. Internal operator notes also stay out of the user-facing read model.

## User-Facing Projection

`projection.review_status_views` is derived and rebuildable. It exposes only stable user-facing states such as:
- `pending_review`
- `under_review`
- `evidence_requested`
- `sealed_or_restricted`
- `appeal_available`
- `appeal_submitted`
- `decided`
- `closed`

User-facing reason codes are constrained by the database and service validation. They must not expose raw accusations, private claims, operator identities, internal evidence details, or safety-sensitive classifications.

## Deferred Scope

ISSUE-13 and ISSUE-14 integration points are intentionally left as consumers:
- room progression may read review status later, but no room progression state machine is implemented here.
- Promise UI may link to proof missing fallback and appeal surfaces later, but no Promise creation, completion, reflection, or dispute-center UI is implemented here.

Proof persistence, full dispute-center UI, settlement UI, and broad operator console product work remain outside this baseline.
