# Backend Schema Skeleton

This note records the Day 1 schema skeleton introduced for M1 Issue #3.

The migration strategy is intentionally small:
- plain PostgreSQL DDL files under `apps/backend/migrations/`
- ordered, deterministic filenames
- no ORM layer
- a runtime migration runner now records checksums in `public.musubi_schema_migrations`

The purpose is to make MUSUBI's physical truth boundaries explicit before Issue #4 and Issue #5 add domain and orchestration behavior.

## Ownership

### `core`
Owns:
- mutable account envelopes
- mutable person-facing profile records
- future raw PII governed by deletion and compliance workflows

Must not own:
- ledger postings
- balances
- Social Trust truth
- outbox delivery state

### `dao`
Owns:
- Promise coordination facts
- settlement coordination facts
- realm-scoped, pseudonymous references used to coordinate state progression
- operator review cases, evidence bundles, appeal cases, and append-only operator decision facts that reference writer-owned source facts without overwriting them
- room progression tracks and append-only room progression facts for Intent, Coordination, Relationship, and Sealed Room surface state

Must not own:
- immutable financial postings
- provider callbacks
- outbox delivery state
- profile data

### `ledger`
Owns:
- append-only journal entries
- append-only account postings
- money values stored as integer minor units
- pseudonymous account references that can point to either Ordinary Account or Controlled Exceptional Account records

Must not own:
- raw PII
- mutable profile fields
- delivery retry state
- projection-only convenience data

### `outbox`
Owns:
- coordination logs for producer delivery
- consumer inbox dedupe records
- retry and quarantine placeholders
- durable message contracts with explicit schema versioning and writer-owned replay ordering

Must not own:
- authoritative business truth
- profile data by convenience
- immutable ledger postings

### `projection`
Owns:
- rebuildable Promise read models
- rebuildable settlement read models
- rebuildable bounded trust read models
- rebuildable user-facing review status models derived from operator review and appeal facts
- rebuildable user-facing room progression models derived from room progression facts and safe ISSUE-12 review posture
- freshness, lag, watermark, and rebuild metadata for projections

Must not own:
- authoritative write decisions
- raw PII
- append-only ledger truth
- raw callback payloads
- raw evidence locators or internal operator notes
- ranking, leaderboard, popularity, or recommendation truth

## Foundation alignment

This skeleton matches the pinned foundation law in three important ways:
- PostgreSQL remains the business truth boundary; Issue #8 now adds runtime checks around this schema.
- mutable PII-bearing records are physically separated from immutable ledger truth.
- outbox and projection data are explicitly non-authoritative.

Cross-boundary joins use pseudonymous UUIDs rather than raw profile fields.
The ledger and outbox schemas intentionally avoid names, bios, birth dates, and other mutable person-facing fields.
The ledger schema uses neutral `account_id` references so append-only truth does not assume only Ordinary Account participants.
The outbox schema requires both `schema_version` and `causal_order` so later workers do not drift into timestamp-only replay semantics.

Money safety is also explicit:
- no `float`
- no `real`
- no `double precision`
- money columns use `BIGINT` fields named `*_minor_units`

`BIGINT` minor units were chosen over `NUMERIC(p,s)` here because the Day 1 skeleton only needs a deterministic, scale-safe storage contract without introducing currency-specific decimal semantics yet.

## Intentionally incomplete

Issue #3 did not implement:
- an ORM
- `SettlementBackend` trait work from Issue #4
- provider adapters or callbacks beyond the current PoC app glue
- outbox/inbox workers, pruning jobs, or retry executors
- happy-route feature expansion

That incompleteness is deliberate.
Issue #3 only establishes the physical ownership boundaries so later issues can build without collapsing core, dao, ledger, outbox, and projection into convenience tables.

## ISSUE-12 operator review additions

Design source: ISSUE-12-operator-review-appeal-evidence.md

The GitHub issue number is intentionally not hardcoded.

Migration `0014_operator_review_appeal_evidence.sql` adds the baseline operator review workflow:
- `core.operator_role_assignments`
- `dao.review_cases`
- `dao.evidence_bundles`
- `dao.evidence_access_grants`
- `dao.operator_decision_facts`
- `dao.appeal_cases`
- `projection.review_status_views`

Migration `0015_operator_review_hardening.sql` adds payload hash columns for review case, operator decision, and appeal idempotency replay checks. The hashes keep mismatch detection durable while avoiding unnecessary reads of internal notes, raw-adjacent summaries, or source snapshots during replay rejection, while legacy rows can self-heal by backfilling the missing hash on first replay.

The architectural boundary is strict: operator decisions are append-only facts and do not rewrite the original Promise, settlement, proof, or source writer truth. User-facing review status is projected from review, decision, evidence, and appeal facts using bounded status and reason codes.

ISSUE-13 room progression and ISSUE-14 Promise UI are future consumers of this boundary. They are not implemented by this schema addition.

## ISSUE-13 room progression additions

Design source: ISSUE-13-room-progression.md

The GitHub issue number is intentionally not hardcoded.

Migration `0016_room_progression_surface.sql` adds the baseline room progression surface:
- `dao.room_progression_tracks`
- `dao.room_progression_facts`
- `projection.room_progression_views`

Room progression tracks preserve the stable realm-scoped participant envelope. Room progression facts are append-only writer facts for transitions between Intent Room, Coordination Room, Relationship Room, and Sealed Room fallback. These facts may reference ISSUE-12 review cases, but they do not duplicate review/evidence/appeal storage and they do not overwrite Promise or settlement writer truth.

`projection.room_progression_views` is a bounded user-facing read model. It is rebuilt from writer-owned room progression facts and may include safe review posture derived from ISSUE-12 projection for display only. State-changing room progression decisions must read writer-owned `dao` facts, not projection rows.

ISSUE-14 Promise UI is a future consumer of this room progression surface. It is not implemented by this schema addition.
