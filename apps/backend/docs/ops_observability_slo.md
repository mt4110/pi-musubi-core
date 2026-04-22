# Ops Observability SLO Skeleton

Design source: ISSUE-17-observability-slo-prompt-pack.

The GitHub issue number is intentionally not hardcoded. This note describes the
internal-only observability surface added for backend health, readiness, and
redacted SLO posture.

## Boundary

Ops observability is not business truth.

The endpoints in this surface only report derived operational posture. They do
not create, rebuild, repair, or advance writer-owned state. Projection lag is
reported for operators as a freshness signal only; state-changing writer
decisions must continue to read writer-owned `dao`, `ledger`, `core`, and
outbox/inbox truth.

`GET` handlers in this surface are side-effect free. They do not enqueue work,
refresh projections, mutate migration tracking, or write audit rows.

## HTTP Surface

Internal/debug-gated:

- `GET /api/internal/ops/health`
- `GET /api/internal/ops/readiness`
- `GET /api/internal/ops/observability/snapshot`
- `GET /api/internal/ops/observability/slo`

These routes are intentionally not participant-facing. In debug builds, they
follow the existing internal route posture, but a participant bearer token is
not accepted as an ops credential. In release, the existing internal bearer-token
requirement applies. They are registered independently from the orchestration
drain endpoint, so disabling the drain worker does not remove the read-only ops
surface.

Readiness reads migration tracking and local migration files without probing or
taking the migration advisory lock. This keeps frequent readiness checks from
contending with `db migrate`.

## Reported Posture

The redacted snapshot reports:

- database connectivity
- projection freshness / lag summaries
- operator review queue aggregate state
- Realm bootstrap review-trigger aggregate state
- orchestration outbox/inbox backlog aggregate state
- unsupported SLI markers

Day 1 SLO summaries use these posture states:

- `ok`: supported signal is present and within threshold.
- `warning`: supported signal crossed its warning threshold.
- `critical`: supported signal crossed its critical threshold.
- `unknown`: the schema or persisted counter needed to report the metric is
  unavailable.

The snapshot top-level `status` aggregates the worst supported status using:

```text
critical > warning > ok
```

Optional unsupported metrics reported as `unknown` do not make the top-level
snapshot critical. If all supported metrics are `ok` and only unsupported
optional metrics are `unknown`, the snapshot remains `ok`.

Day 1 thresholds:

| Signal | Warning | Critical |
| --- | ---: | ---: |
| Projection lag | `>= 60_000 ms` | `>= 1_800_000 ms` |
| Operator review queue oldest open case | `>= 86_400_000 ms` | `>= 259_200_000 ms` |
| Realm review trigger oldest open trigger | `>= 86_400_000 ms` | `>= 259_200_000 ms` |
| Orchestration backlog oldest pending/processing item | `>= 300_000 ms` | `>= 1_800_000 ms` |

Projection summaries report `unknown` when the projection table or freshness
columns are missing. Empty projection tables report `ok`; populated tables are
classified from max projection lag.

Queue and backlog summaries report `unknown` when their source tables are
missing. Empty queues report `ok`; non-empty queues are classified from the
oldest open, pending, or processing item.

Unsupported metrics return `unknown`. They do not return fake `0` values. For
example, idempotency replay mismatches are rejected by writer-boundary checks,
but the current schema does not persist a dedicated mismatch counter, so that
SLI is reported as `unknown`.

## Redaction

The snapshot must not expose:

- raw evidence locators
- raw callback payloads
- operator notes
- operator identities
- source fact identifiers
- source fact counts
- raw source snapshots
- moderation internals
- sealed fallback internals
- raw PII

Operator-facing queue data is aggregate and redacted. It is intended to help a
human operator notice backlog or freshness problems, not to inspect case
contents.

## Deferred Scope

This skeleton does not add:

- external observability vendor integration
- metrics scraping protocol selection
- alert delivery
- incident automation
- participant-facing ops UI
- projection rebuild or repair actions
- Promise proof persistence
- Promise completion writer truth
- reimplementation of ISSUE-12, ISSUE-13, ISSUE-14, or ISSUE-15
