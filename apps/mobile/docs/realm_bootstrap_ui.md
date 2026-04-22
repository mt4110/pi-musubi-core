# Realm Bootstrap UI

Design source: ISSUE-15 realm bootstrap / admission / sponsor-backed early growth.

This UI is intentionally narrow:
- authenticated participants can submit a Realm creation request
- participants can check a participant-safe bootstrap summary by `realm_id`
- the operator / Steward panel is a redacted display surface only

The Flutter Web client does not embed internal operator bearer tokens. Internal
review actions stay on backend/operator-gated routes and are not made available
from the participant web surface.

The UI must not imply:
- guaranteed admission
- public self-serve Realm issuance
- referral growth
- ranking or recommendation boost
- DM unlock
- paid romantic advantage

Projection rows displayed here are read models. Writer-owned Realm,
sponsor, corridor, and admission decisions remain backend/PostgreSQL truth.
