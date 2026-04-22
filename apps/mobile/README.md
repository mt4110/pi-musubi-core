# musubi_mobile

Flutter Web client for the Pi-first MUSUBI Day 1 surface.

## Promise UI baseline

Design source: ISSUE-14-promise-ui-baseline.md

The match detail screen now creates a Promise through the backend writer-owned
`POST /api/promise/intents` API when API repositories are enabled. The previous
local payment/deposit success stub is no longer treated as Promise creation.

After creation, the app routes to `/promises/:promiseIntentId` and displays
participant-safe Promise / settlement / proof status from projection endpoints
when those projections are available. The completion area is informational only:
it does not claim completion from a local button, and it does not mutate trust,
settlement, room progression, reward, ranking, or unlock state.

The current discovery cards remain static demo fixtures. When API repositories
are enabled, Promise creation only succeeds if the referenced counterparty
account already exists in the backend database; otherwise the UI shows bounded
unavailable copy instead of inventing Promise truth on the client.

See `docs/promise_ui_baseline.md` for the ISSUE-14 boundary.

## Realm bootstrap UI baseline

Design source: ISSUE-15 realm bootstrap / admission / sponsor-backed early growth.

The `/realms/bootstrap` screen adds the minimal participant-side Realm request
form and participant-safe bootstrap summary surface. It does not make Realm
creation self-serve, and it does not expose internal operator notes, source
facts, review trigger context, or raw evidence.

The operator / Steward panel is a redacted review surface. Internal approval,
rejection, sponsor-record, admission, and review-summary APIs remain
backend-gated and are not unlocked by client state.

See `docs/realm_bootstrap_ui.md` for the ISSUE-15 UI boundary.
