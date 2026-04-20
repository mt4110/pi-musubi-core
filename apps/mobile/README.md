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

See `docs/promise_ui_baseline.md` for the ISSUE-14 boundary.
