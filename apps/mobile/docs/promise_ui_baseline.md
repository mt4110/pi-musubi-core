# Promise UI Baseline

Design source: ISSUE-14-promise-ui-baseline.md

The GitHub issue number is intentionally not hardcoded. The design source number is a product/design reference, not a repository issue identifier.

ISSUE-14 adds the first participant-facing Promise creation and status surface for the current Flutter Web app. It builds on the accepted ISSUE-12 review/evidence baseline and the accepted ISSUE-13 room progression baseline. It does not reimplement either surface.

## Boundary

Promise creation uses the backend writer-owned API:

- `POST /api/promise/intents`

The mobile app does not create local Promise truth. The match detail screen keeps a stable idempotency key for the current participant action, so retrying the same button action can replay safely without claiming a duplicate success.

The Promise status screen reads participant-safe projection endpoints when available:

- `GET /api/projection/promise-views/{promise_intent_id}`
- `GET /api/projection/settlement-views/{settlement_case_id}/expanded`

Projection rows are display data only. The UI must not use projection state to make writer-owned decisions.

The current discovery cards are still static demo fixtures, not writer-owned
match truth. In API mode, Promise creation only succeeds when the referenced
counterparty account already exists in the backend writer DB. If that setup is
missing, the UI must show calm unavailable copy instead of fabricating Promise
truth client-side.

## Completion Posture

The completion/proof area is intentionally informational in this baseline. It does not expose a local "complete Promise" mutation and does not update trust, settlement, room progression, reward, ranking, or unlock state.

If proof status is unavailable, the UI says so directly. When a future writer-owned proof/completion endpoint exists, this screen can link to it without changing the current boundary.

## Privacy

Participant-facing UI models intentionally omit:

- raw evidence locators
- operator IDs
- internal operator notes
- review source fact IDs
- raw source snapshots
- decision payload internals
- sealed fallback internals

ISSUE-12 review and evidence internals remain behind the operator/review boundary. ISSUE-13 room progression remains a safe projection consumer only.

## Deferred

This baseline does not implement:

- Promise confirm / complete / reflect writer mutations
- proof persistence
- room progression writer changes
- raw evidence retrieval
- dispute center UI
- ranking, leaderboard, recommendation boost, swiping changes, or DM unlock behavior
- native mobile platform work
