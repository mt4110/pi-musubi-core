# Day 1 Launch Posture

Design source: ISSUE-18 launch posture / Day 1 pilot.

The GitHub issue URL is intentionally not hardcoded.

## Purpose

ISSUE-18 keeps Day 1 launch bounded by server policy. Launch posture is not
participant UI state, projection state, or observability state. It is parsed
from server-owned configuration and enforced on participant write routes.

This baseline exists to prevent accidental public launch while Realm bootstrap,
operator review, and observability are still pilot-oriented.

## Launch Modes

`MUSUBI_LAUNCH_MODE` accepts:

- `closed`
- `pilot`
- `paused`

Missing `MUSUBI_LAUNCH_MODE` defaults to `closed`. Invalid values fail closed
and are reported only through the internal launch posture endpoint as
`invalid_launch_mode`.
`open_preview` is unsupported for Day 1 production launch config and also
fails closed, with an internal-only warning.

Mode behavior:

- `closed`: participant writes are blocked.
- `pilot`: allowlisted cohort members may use gated participant flows.
- `paused`: participant writes are blocked regardless of allowlist.

Day 1 does not include a general public/open launch mode. The global launch
bound is the configured pilot allowlist plus existing writer-owned Realm
admission, corridor, and sponsor policy bounds.

## Config Keys

All keys are optional unless noted.

| Key | Default | Invalid value behavior |
| --- | --- | --- |
| `MUSUBI_LAUNCH_MODE` | `closed` | fail closed |
| `MUSUBI_LAUNCH_ALLOWLIST_PI_UIDS` | empty | empty members ignored |
| `MUSUBI_LAUNCH_ALLOWLIST_ACCOUNT_IDS` | empty | empty members ignored |
| `MUSUBI_LAUNCH_SUPPORT_CONTACT_URL` | unset | support contact omitted |
| `MUSUBI_LAUNCH_SUPPORT_CONTACT_LABEL` | unset | support contact omitted |
| `MUSUBI_KILL_SWITCH_AUTH` | `false` | fail closed for auth |
| `MUSUBI_KILL_SWITCH_PROMISE_CREATION` | `false` | fail closed for Promise creation |
| `MUSUBI_KILL_SWITCH_PROOF_CHALLENGE` | `false` | fail closed for proof challenge |
| `MUSUBI_KILL_SWITCH_PROOF_SUBMISSION` | `false` | fail closed for proof submission |
| `MUSUBI_KILL_SWITCH_REALM_REQUESTS` | `false` | fail closed for Realm requests |
| `MUSUBI_KILL_SWITCH_REALM_ADMISSIONS` | `false` | fail closed for Realm admissions |

Boolean switches parse `1/0`, `true/false`, `yes/no`, and `on/off`
case-insensitively. Any other value turns that switch on and adds an internal
config warning.

Allowlist members are never returned by API responses. Internal posture may
return counts only.

For token-authenticated participant writes, pilot allowlist checks accept either
the account id or the linked Pi UID from the authorized session. Internal
target-account gates that do not carry a participant session only evaluate the
target account id.

## HTTP Surface

Public, side-effect-free:

- `GET /api/launch/posture`

Internal/debug-gated, side-effect-free:

- `GET /api/internal/launch/posture`

Public response:

```json
{
  "launch_mode": "closed",
  "participant_posture": "closed",
  "message_code": "launch_closed",
  "support_contact": null,
  "generated_at": "timestamp"
}
```

Internal response:

```json
{
  "launch_mode": "closed",
  "effective_posture": "closed",
  "config_warnings": [],
  "kill_switches": {
    "auth": false,
    "promise_creation": false,
    "proof_challenge": false,
    "proof_submission": false,
    "realm_requests": false,
    "realm_admissions": false
  },
  "allowlist": {
    "source": "none",
    "pi_uid_count": 0,
    "account_id_count": 0,
    "members_visible": false
  },
  "support_contact_configured": false,
  "observability_is_launch_truth": false,
  "projection_is_launch_truth": false,
  "generated_at": "timestamp"
}
```

## Route Gates

Server-side launch gates protect:

- `POST /api/auth/pi`
- `POST /api/promise/intents`
- `POST /api/proof/challenges`
- `POST /api/proof/submissions`
- `POST /api/realms/requests`
- `POST /api/internal/realms/{realm_id}/admissions`

The auth gate uses `pi_uid` before an account exists. Post-auth participant
gates use the authenticated `account_id`. Client-provided launch mode,
allowlist state, or UI state is ignored.
Internal Realm admission is a new admission write, so it is launch-gated using
the target participant account from the request payload. It cannot bypass
`closed`, `pilot`, `paused`, or the Realm admission kill switch.

Blocked participant responses are bounded:

```json
{
  "error": "launch_paused",
  "message_code": "launch_paused"
}
```

Route-level kill switch message codes:

- `auth_paused`
- `promise_creation_paused`
- `proof_challenge_paused`
- `proof_submission_paused`
- `realm_request_paused`
- `realm_admission_paused`

Mode block message codes:

- `launch_closed`
- `launch_pilot_not_allowed`
- `launch_paused`

## Redaction Boundary

Participant responses must not expose:

- allowlist members
- raw `pi_uid` lists
- raw account id lists
- operator ids
- operator notes
- review internals
- source fact ids
- source fact counts
- evidence internals
- incident internals
- moderation internals
- sealed fallback internals
- raw PII

The internal posture response still exposes counts only, never member lists.

## Relationship To Observability

ISSUE-17 observability can inform humans, but it does not open or close launch
posture. SLO status, projection lag, readiness, and projection rows are not
launch truth.

Internal ops/readiness endpoints remain available during participant launch
pause according to their existing internal/debug gate.

## Deferred Scope

This baseline does not add:

- ISSUE-16 reconciliation / PITR / failover
- dynamic launch flag admin UI
- external alert integrations
- public launch
- referral or growth programs
- paid priority access
- ranking or recommendation boosts
- DM unlock behavior
- Promise proof persistence
- Promise completion writer truth
