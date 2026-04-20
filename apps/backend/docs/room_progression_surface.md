# Room Progression Surface

Design source: ISSUE-13-room-progression.md

The GitHub issue number is intentionally not hardcoded. The design source number is a product/design reference, not a repository issue identifier.

ISSUE-13 adds the backend baseline for calm participant-facing room progression:
- Intent Room
- Coordination Room
- Relationship Room
- Sealed Room fallback

ISSUE-12 operator review / appeal / evidence is already the review foundation. Room progression consumes that boundary when sealed fallback references a review case. It does not duplicate review cases, evidence bundles, access grants, appeals, or operator decision storage.

ISSUE-14 Promise UI is a future consumer of this surface. It is not implemented by this backend baseline.

## Boundary

Room progression writer truth lives in `dao`:
- `dao.room_progression_tracks` stores the stable progression envelope, participants, current visible stage, and the last bounded user-facing state.
- `dao.room_progression_facts` stores append-only progression facts. These facts are the durable transition history and may reference a writer-owned ISSUE-12 `review_case_id`.

User-facing state lives in `projection.room_progression_views`. The projection is rebuildable and exists for display only.

State-changing decisions must read writer-owned facts, not projection rows. For example, sealing a live room, restore from a sealed room, and reviewed restriction follow-up all read room-scoped ISSUE-12 review truth rather than projection rows. `projection.review_status_views` may shape display posture, but it is not authoritative for restore or progression decisions.

Room progression facts must not mutate Promise, settlement, proof, or operator review writer truth.

## State Shape

The ordinary path is intentionally small:

```text
Intent Room -> Coordination Room -> Relationship Room
```

Skipped transitions are rejected unless a later design explicitly changes the state machine. Sealed fallback is supported from any visible stage:

```text
Intent Room       -> Sealed Room
Coordination Room -> Sealed Room
Relationship Room -> Sealed Room
```

Sealed fallback is a calm safety/product posture, not a punishment narrative.
While a room remains sealed, later writer-owned facts may keep `sealed -> sealed` to record
follow-up posture such as `sealed_under_review -> sealed_restricted` without reopening the room.
Live-room seal transitions require an active, room-scoped `sealed_room_fallback` review case so
the participant-facing under-review posture always has writer-owned backing and does not reuse a
stale decided review cycle.
Restore transitions are operator-triggered only and must return the room to the live stage it
held immediately before the current sealed fallback began.
Reviewed restriction outcomes are also not participant-attributed: a `sealed_restricted` fact must
be recorded as operator- or system-triggered once a writer-owned restrict decision exists.

## User-Facing Projection

`projection.room_progression_views` exposes bounded participant-facing fields:
- room progression id
- realm id
- participant account ids for authorization
- visible stage
- status code
- user-facing reason code
- safe review posture fields when linked to ISSUE-12 review state
- source watermark, source fact count, projection lag, and rebuild generation

`rebuild_generation` is a monotonic `BIGINT` within this single projection surface. Unlike the
UUID rebuild generations used by multi-projection happy-route rebuilds, it only needs to express
local refresh ordering for room progression views. Full room-progression rebuilds allocate that
ordinal inside a single writer transaction guarded by a database advisory lock, so the local
generation stays race-free without pretending to be shared cross-projection provenance.

It does not expose:
- raw evidence locators
- internal operator notes
- operator identities
- raw source snapshots
- decision payload internals
- internal safety classifications
- accusatory labels or private claims

## Idempotency

Room progression create and transition writes use durable idempotency keys and canonical payload hashes.
Fact appends reject missing `fact_idempotency_key` values so retry safety is always explicit at the
writer boundary.

Reusing the same idempotency key with the same semantic JSON payload is a replay. Object key ordering does not change the payload hash. Reusing the same key with a different semantic payload is rejected.

## Deferred Scope

This baseline intentionally does not implement:
- ISSUE-12 review / appeal / evidence workflow again
- ISSUE-14 Promise creation, completion, proof-missing fallback UI, or reflection UI
- Flutter or mobile UI
- settlement UI
- full dispute center UI
- proof persistence
- raw evidence retrieval endpoints
- broad operator console product work
- recommendation, ranking, swiping, engagement-loop, or DM-unlock behavior
