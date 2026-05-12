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
- realm creation requests, sponsor-backed bootstrap records, bootstrap corridors, realm admissions, and internal review triggers for bounded Day 1 realm growth

Must not own:
- immutable financial postings
- provider callbacks
- outbox delivery state
- profile data

### `social_trust`
Owns:
- C1 Social Trust proposed mutation attempt intake records
- C1 Social Trust intake decision records for rejected attempts and `CandidateForWriterPersistence`
- database-enforced idempotency / replay posture for proposed mutation attempts
- minimized reason, evidence-posture, reviewability, retention-class, and audit metadata for intake decisions

Must not own:
- actual Social Trust mutation facts
- Social Trust scores, weights, ranks, display levels, narrowing, freeze, or recovery facts
- Relationship Depth facts
- projection refresh state
- raw PII, raw evidence payloads, payment behavior, popularity metrics, engagement telemetry, operator notes, support tickets, or issue comments

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
- rebuildable participant-safe realm bootstrap and admission views
- rebuildable operator-safe realm bootstrap health summaries
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

Migration `0017_room_progression_actor_consistency.sql` adds the actor-consistency constraint that
governs room progression write validity, so both migrations are part of the schema surface
operators and developers must apply for ISSUE-13 writes.

Room progression tracks preserve the stable realm-scoped participant envelope. Room progression facts are append-only writer facts for transitions between Intent Room, Coordination Room, Relationship Room, and Sealed Room fallback. These facts may reference ISSUE-12 review cases, but they do not duplicate review/evidence/appeal storage and they do not overwrite Promise or settlement writer truth.

`projection.room_progression_views` is a bounded user-facing read model. It is rebuilt from writer-owned room progression facts and may include safe review posture derived from ISSUE-12 projection for display only. State-changing room progression decisions must read writer-owned `dao` facts, not projection rows.

ISSUE-14 Promise UI is a future consumer of this room progression surface. It is not implemented by this schema addition.

## ISSUE-15 realm bootstrap and admission additions

Design source: ISSUE-15-realm-bootstrap-and-admission.md

The GitHub issue number is intentionally not hardcoded.

Migration `0018_realm_bootstrap_admission.sql` adds the bounded realm bootstrap baseline:
- `dao.realm_requests`
- `dao.realms`
- `dao.realm_sponsor_records`
- `dao.bootstrap_corridors`
- `dao.realm_admissions`
- `dao.realm_review_triggers`
- `projection.realm_bootstrap_views`
- `projection.realm_admission_views`
- `projection.realm_review_summaries`

The architectural boundary is strict:
- realm creation requests do not become public self-serve realm issuance
- sponsor authority is explicit, quota-bounded, auditable, and revocable
- corridor expiry, corridor caps, sponsor quota, and restricted/suspended realm blocking are enforced server-side on writer truth
- participant-safe projections are rebuildable convenience views and do not expose operator IDs, raw review triggers, source fact IDs/counts, or moderation internals
- operator/steward summaries are redacted, rebuildable, and still non-authoritative

ISSUE-12 operator review remains the review/evidence system of record, ISSUE-13 room progression remains the room surface baseline, and ISSUE-14 Promise UI remains out of scope for this schema addition.

## C1 Social Trust intake persistence additions

Design source: `docs/readiness/c1_social_trust_intake_handoff_gate_decision.md` in `musubi-foundation` PR #92.

Migration `0020_social_trust_intake_persistence.sql` adds the narrow C1 Social Trust intake persistence baseline:
- `social_trust.proposed_mutation_attempts`
- `social_trust.intake_decisions`

The architectural boundary is strict:
- proposed mutation attempts and intake decisions are writer-owned intake / replay facts only
- `CandidateForWriterPersistence` is internal intake classification only
- durable idempotency is enforced by a PostgreSQL unique index over the minimized dedupe identity
- duplicate delivery with payload drift fails closed through stored payload hashes
- retention is mapped to the ADR-0012 category `Social Trust evidence or future Social Trust writer facts` without inventing concrete retention durations
- rejected attempts and candidates do not emit projection refresh work
- no Social Trust mutation fact, score, weight, rank, display level, Relationship Depth fact, public API, or mobile UI is introduced
