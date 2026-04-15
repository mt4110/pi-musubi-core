# Safer Venue Proof Primitives

ISSUE-10 adds Day 1 proof-input primitives for venue and real-world proof flows.

This is not a final anti-spoofing system.
It is a bounded input-fact boundary that makes naive static QR / raw GPS proof harder to accidentally treat as truth.

## HTTP surface

- `POST /api/proof/challenges`
- `POST /api/proof/submissions`

Both endpoints require the same bearer session used by the happy-route API.

### Challenge request

`POST /api/proof/challenges` creates a short-lived venue challenge for the authenticated account.

Accepted fields:

- `venue_id`
- `realm_id`
- `fallback_mode`

The public subject-facing endpoint supports only the normal venue-code flow.
`fallback_mode` may be omitted or set to `none`.
`fallback_mode=operator_pin` returns `400 Bad Request` because the request is asking the public subject route to perform an operator-only action.
The public request body is never a source of truth for `operator_id`, audit identity, or fallback rate-limit accounting.
The response deliberately reports `operator_pin_issued = false`; it does not expose an operator PIN or operator delivery object.

The service still has an internal operator fallback primitive for the future authenticated operator surface.
The clear PIN is generated from server-only entropy and returned only through a separate service-layer operator delivery object.
That internal primitive must stay unreachable from subject-facing HTTP until an authenticated operator principal exists.

### Proof submission request

`POST /api/proof/submissions` records and verifies a proof envelope.

Accepted fields:

- `challenge_id`
- `venue_id`
- `display_code`
- `key_version`
- `client_nonce`
- `observed_at_ms`
- `coarse_location_bucket`
- `device_session_id`
- `fallback_mode`
- `operator_pin`

The handler denies unknown JSON fields.
Raw latitude, longitude, advertising IDs, hardware fingerprints, and similar high-risk device facts are intentionally outside this Day 1 envelope.
`fallback_mode` is parsed again at the service boundary.
Only blank, `none`, and `operator_pin` are recognized; unsupported non-empty values are rejected before display-code verification and are stored only as the bounded sentinel `unsupported`.
`coarse_location_bucket` is sanitized before any proof record is built.
Only lowercase ASCII bucket labels like `tokyo-shibuya` are retained.
Coordinates, exact-address-like values, oversized strings, and other invalid hints are dropped before persistence and represented only by the `invalid_coarse_location_hint` risk flag plus a bounded invalid marker in the redacted payload.

## Verification boundary

The server verifies:

- challenge existence
- challenge TTL
- single-use consumption
- account / venue binding
- client nonce binding
- challenge-issued venue key version and key status
- rotating display-code validity for the current or immediately previous short window
- coarse location hint shape
- fallback mode canonical value
- optional operator PIN fallback
- replay of an identical server-keyed replay key
- bounded failed attempts per challenge

Outcomes are:

- `verified`
- `rejected`
- `quarantined`

These records are proof-input facts only.
They are not settlement truth, ledger truth, Social Trust truth, or attendance truth by themselves.
Later product flows must explicitly decide what a verified proof can unlock.

## Key-version-aware verification

Each `(realm_id, venue_id)` pair has an active key version in the Day 1 in-memory store.
The display code is derived from:

- venue secret material
- realm id
- venue id
- key version
- short server-time window

Venue secret material is derived from the server-only `PROOF_MASTER_SECRET`.
If `PROOF_MASTER_SECRET` is absent, the Day 1 in-memory stand-in generates a process-local random server secret.
The rotating code uses an HMAC-style keyed function; realm id, venue id, and key version alone are not enough to compute it.
The same `venue_id` in two different Realms therefore has separate active key state and separate valid display codes.

Verification uses the key version issued with the challenge as the source of truth.
The submitted `key_version`, when present, is only an echo check; a mismatch is rejected as `key_version_mismatch` and cannot select another key.
Active, draining, and revoked semantics are evaluated against the challenge-issued key version only.
This gives key rotation an explicit seam without claiming production key-management completeness.

## Replay and TTL baseline

Challenges expire after a short TTL and are consumed on successful verification.
Exact proof-envelope replay is rejected using a deterministic server-keyed replay key.
Replay keys include the authenticated subject plus canonicalized envelope fields and are HMACed with the server secret so stored replay material cannot be used by itself to enumerate low-entropy display codes or operator PINs offline.
Each issued challenge also has a small failed-attempt budget.
Malformed envelopes, unsupported fallback modes, missing nonce, missing challenge, subject mismatch, venue mismatch, replay, expired challenges, risk quarantine, and key-version echo mismatch do not consume that budget.
Only bound secret-check failures, such as a wrong display code or wrong operator PIN after challenge, subject, venue, nonce, and live-status binding, consume attempts.
After the budget is exhausted through those real secret-check failures, the challenge is quarantined and the client must request a new challenge.

This is a baseline, not a complete abuse-defense model.
It is designed so the future PostgreSQL version has obvious uniqueness and retention boundaries.

## Operator fallback

`operator_pin` fallback is for degraded venue moments, not the normal path.
It is internal/deferred in the current HTTP app.
Subject-facing challenge creation cannot request it, cannot self-assert an operator identity, cannot create operator issuance audit rows, and cannot burn an operator fallback rate-limit budget.

The internal primitive keeps the intended shape for a future authenticated operator route.
PIN issuance is:

- tied to a challenge
- tied to an operator id
- generated from server-only entropy
- short-lived
- audited
- rate-limited per venue / operator

Successful fallback adds an `operator_fallback` risk flag.
The audit record keeps the PIN hash and issuance metadata, not the clear PIN.
The clear PIN is not stored in proof state and is not part of the client-facing response.

## Stored data

Proof submissions persist:

- proof submission id
- challenge id
- subject account id
- server-keyed replay key
- server-keyed display code hash
- received timestamp
- optional client observed timestamp
- sanitized coarse location bucket only
- server-keyed device session hash
- canonical fallback mode
- redacted payload shape
- verification status

They do not store raw GPS coordinates, exact addresses, oversized location strings, arbitrary fallback-mode strings, exact device fingerprint material, clear display codes, clear operator PINs, or plain digests over low-entropy display-code / operator-PIN material.
In-memory proof submissions, verifications, and replay material are pruned by TTL and max-entry caps so rejected-envelope evidence does not grow process memory without bound.

## Residual risk

Residual risk remains explicit:

- coarse location hints can still be spoofed
- display-code observation can still be relayed by a colluding or compromised client
- operator fallback depends on human operational discipline
- in-memory Day 1 state must become PostgreSQL-backed before production use
- deployments need explicit `PROOF_MASTER_SECRET` custody instead of process-local fallback
- operator PIN delivery still needs a real authenticated operator channel
- production venue key custody and rotation policy are still deferred

Product wording must not claim complete anti-spoofing.
The accurate claim is that the system records a bounded, server-verifiable proof input with visible residual risk.
