# Foundation Lock

Status: Draft; aligned to accepted foundation commit `0bdbde0`
Applies to: `mt4110/pi-musubi-core`
Purpose: Pin the constitutional and architectural source of truth that this implementation repository must follow.

---

## 0. Why this file exists

`pi-musubi-core` is not allowed to “freestyle” product meaning.
This file pins the upstream MUSUBI design corpus that implementation must obey.

The implementation repo may move quickly.
The foundation repo must move carefully.
This file keeps them aligned.

---

## 1. Upstream source of truth

Upstream repository:

- `mt4110/musubi-foundation`

Pinned reference for implementation work:

- Foundation reference type: `commit`
- Foundation commit SHA: `0bdbde0da0c9a2838d814975df3888868cf9f892`
- Foundation commit title: `docs: accept Deletion Reset Boundary foundation ADR`
- Date pinned: `2026-05-03`
- Pinned by: `Masaki Takemura`
- Pinned commit URL: `https://github.com/mt4110/musubi-foundation/commit/0bdbde0da0c9a2838d814975df3888868cf9f892`
- Previous pinned reference: `22fcd261` / `docs: accept Key-Shredding foundation ADR`

No release tag is asserted for this alignment.
Do not invent a foundation version label for this commit.

Do not update this file casually.
If the implementation starts depending on a newer foundation decision, update the pinned reference explicitly.

---

## 2. Reading order for implementers and agents

Before coding, read these upstream documents in order.

### Constitutional layer
1. `docs/00_constitution.md`
2. `docs/glossary.md`
3. `docs/term_decisions.md`

### ADR layer
4. `docs/adr/0001_postgres_truth_source.md`
5. `docs/adr/0002_transactional_outbox.md`
6. `docs/adr/0003_server_realm_citadel_pool.md`
7. `docs/adr/0004_japan_first_tax_first_and_named_operator.md`
8. `docs/adr/0005_store_policy_and_token_boundary.md`
9. `docs/adr/0006_writer_truth_projection_authority.md` - Status: Accepted at `0c1c636`
10. `docs/adr/0007_pii_evidence_segregation.md` - Status: Accepted at `0c1c636`
11. `docs/adr/0008_topology_ownership_boundaries.md` - Status: Accepted at `0c1c636`
12. `docs/adr/0009_account_constraints.md` - Status: Accepted at `0c1c636`
13. `docs/adr/0010_promise_trust_depth_semantics.md` - Status: Accepted at `0c1c636`
14. `docs/adr/0011_legal_hold_evidence_preservation_boundary.md` - Status: Accepted at `638c213`
15. `docs/adr/0012_retention_class_registry_pruning_archive_policy.md` - Status: Accepted at `6500c95`
16. `docs/adr/0013_deletion_request_subject_tombstone_reference_preserving_anonymization.md` - Status: Accepted at `14278fc`
17. `docs/adr/0014_key_shredding_boundary_immutable_truth_preservation.md` - Status: Accepted at `22fcd261`
18. `docs/adr/0015_natural_person_ordinary_account_continuity.md` - Status: Accepted at `166cb3a`
19. `docs/adr/0016_anti_abuse_continuity_marker_contract.md` - Status: Accepted at `e364c0a`
20. `docs/adr/0017_age_assurance_writer_state_youth_safety_boundary.md` - Status: Accepted at `8c06963`; appeal / human-review guard clarified at `067cd85`
21. `docs/adr/0018_deletion_reset_boundary_account_lifecycle_after_deletion.md` - Status: Accepted at `0bdbde0`

### Detail layer
22. `docs/detail/accountability_matrix.md`
23. `docs/detail/critical_incident_and_loss.md`
24. `docs/detail/automated_decisioning_and_human_appeal.md`
25. `docs/detail/youth_safety_and_age_assurance.md`
26. `docs/detail/off_platform_handoff_and_scam_prevention.md`
27. `docs/detail/data_deletion_vs_legal_hold.md`
28. `docs/detail/realm_model.md`
29. `docs/detail/data_scope_model.md`
30. `docs/detail/mobility_model.md`
31. `docs/detail/settlement_model.md`
32. `docs/detail/settlement_backend_trait.md`
33. `docs/detail/proof_of_infrastructure.md`
34. `docs/detail/protected_groups_and_translation_safety.md`

### Whitepaper layer (contextual, not higher than detail/ADR)
35. `docs/whitepaper/01_executive_summary.md`
36. `docs/whitepaper/02_realm_model.md`
37. `docs/whitepaper/03_experience_model.md`
38. `docs/whitepaper/04_dm_shield.md`
39. `docs/whitepaper/05_trust_model.md`
40. `docs/whitepaper/06_promise_protocol.md`
41. `docs/whitepaper/07_realm_economy.md`
42. `docs/whitepaper/08_unlock_engine.md`

If any of the above are unavailable or materially inconsistent, stop and escalate.

### ADR-RC and implementation authority

`docs/adr_reconstruction/*` files remain reconstruction records only.
They are not implementation authority unless converted into an accepted foundation ADR and locked here.

`docs/adr_drafts/*` files remain draft records only.
They are not implementation authority unless converted into an accepted foundation ADR and locked here.

Accepted ADR-0006 through ADR-0018 are implementation-authorizing only within their stated scope.
ADR-0011 through ADR-0014 complete the Data Lifecycle foundation tranche for foundation scope only.
ADR-0015 through ADR-0018 complete the Account Lifecycle foundation tranche for accepted foundation scope only.
Runtime implementation is not complete.
Prompt 3 implementation is not globally unblocked.
Prompt 3 implementation may proceed only where all applicable foundation ADRs and dependencies are Accepted.

Implementation merge history, issue order, branch ancestry, and existing code are not foundation design proof.

---

## 3. Non-negotiable implementation laws

The following laws must survive all implementation work.

### 3.1 PostgreSQL is truth
Business truth lives in PostgreSQL.
Providers, chains, callbacks, and external runtimes are observed evidence.
Writer truth controls state-changing decisions.
Projection is not repair authority.
Observability is not writer truth.
Provider events, chain events, callback records, Device Attestation, Proximity Proof, ZK proofs, client state, and caches are evidence only unless an accepted ADR explicitly says otherwise.

### 3.2 PII and immutable truth are physically separated
PII belongs in mutable, deletable, legally governed core records.
Immutable ledger / settlement / outbox truth must use pseudonymous identifiers.

### 3.3 Server / Realm / Citadel / Pool must remain distinct
Never collapse UX alias, logical community, runtime placement, and substrate into one concept.

### 3.4 realm_id is first-class and durable
Realm promotion or relocation must not break realm identity.

### 3.5 One natural person, one Ordinary Account
Do not weaken this.
Operational or system identities are not ordinary participants.
Controlled Exceptional Accounts must not become participants.

### 3.6 Trust is reliability infrastructure
Not popularity.
Not human worth.
Not a bypass around consent.
Not reply speed.
Not dwell time.
Not tenure.
Not payment amount or payment frequency.
Not engagement loops.
Not romantic desirability.

### 3.7 Promise is accountable commitment
Not a right to a person.
Not romance entitlement.

### 3.8 Relationship Depth requires consented facts
Relationship Depth must not increase from unilateral or non-consented facts.

### 3.9 Operator notes are not writer truth
Operator notes are not financial truth, consent truth, Social Trust truth, Relationship Depth truth, or repair authority.

### 3.10 Escrow is self-discipline
Not bounty.
Not pay-to-win.
Not platform extraction from loneliness.

### 3.11 Outbox / inbox are coordination logs
Not eternal truth.
They require pruning, quarantine, retry discipline, and idempotent external delivery.

### 3.12 No float money
All monetary values must use integer minor units or fixed-point-safe abstractions.

### 3.13 Drop-Tx-Before-Await
No authoritative transaction may be held open during external network I/O.

### 3.14 Day 1 is web-only
Implement for Pi App / DApp on the web first.
Do not optimize architecture around native mobile assumptions.

---

## 4. Day 1 implementation cautions

These are the four places where AI-assisted implementation is most likely to create future debt.
They must be watched explicitly.

### 4.1 Initial schema pollution
Do not let `User` or profile tables absorb settlement, balance, ledger, or trust truth by convenience.
Keep mutable person records physically separate from immutable financial and coordination records.

### 4.2 Silent reintroduction of await-inside-tx
Refactors may accidentally widen transaction scopes.
Treat this as a severe defect.
Use code review and tests to catch it.

### 4.3 Coordination-data bloat
Outbox/inbox implementation is incomplete without retention and pruning strategy.
A working publisher without garbage collection is unfinished work.

### 4.4 Weak real-world proof inputs
Do not launch high-value reward or trust updates on top of naive spoofable proofs.
GPS-only and static-QR-only designs are too weak.
Use dynamic, venue-bound, signed, or otherwise spoof-resistant proofs as early as possible.

---

## 5. Immediate implementation intent for `pi-musubi-core`

The current repo begins as a Pi Sign-in + deposit PoC.
Its mission now is to evolve into the canonical MUSUBI implementation monorepo for Day 1 web operation.

The next major implementation package should align to:

1. crate / package boundaries that reflect the foundation docs
2. schema skeleton that preserves physical truth boundaries
3. migration order that avoids irreversible debt
4. settlement-domain primitives with strict type discipline
5. outbox / inbox orchestration with pruning from Day 1
6. proof inputs that are safer than naive client assertions

This lock update does not authorize broad runtime implementation.
Implementation work must still be split into tasks whose applicable foundation ADRs and dependencies are Accepted.

The Data Lifecycle foundation tranche is a FULL FOUNDATION PASS for accepted foundation scope:

- ADR-0011 Legal Hold / Evidence Preservation Boundary
- ADR-0012 Retention Class Registry / Pruning / Archive Policy
- ADR-0013 Deletion Request / Subject Tombstone / Reference-Preserving Anonymization
- ADR-0014 Key-Shredding Boundary / Immutable Truth Preservation

This does not implement Legal Hold, retention workers, deletion workers, Subject Tombstone runtime behavior, or key-shredding runtime behavior.
Prompt 3 remains not globally unblocked.

The Account Lifecycle foundation tranche is a FULL FOUNDATION PASS for accepted foundation scope:

- ADR-0015 Natural Person Uniqueness / Ordinary Account Continuity
- ADR-0016 Anti-Abuse Continuity Marker Contract
- ADR-0017 Age Assurance Writer State / `youth_safety` Legal Hold Boundary
- ADR-0018 Deletion Reset Boundary / Account Lifecycle After Deletion

This does not implement Natural Person uniqueness runtime behavior, Anti-Abuse Continuity Marker runtime behavior, Age Assurance runtime behavior, account deletion reset runtime behavior, re-entry after deletion, Deletion Request runtime behavior, or Subject Tombstone runtime behavior.
Prompt 3 remains not globally unblocked.

---

## 6. Update protocol

Update this file when:

- foundation docs reach a new canonical release
- implementation depends on a newer ADR or term decision
- a durable architectural decision changes upstream

When updating:

1. change the pinned SHA / release reference
2. summarize what changed
3. verify AGENTS.md still matches the new foundation
4. run a drift review before merging

### Drift note template
- Updated from foundation SHA: `<old>` -> `<new>`
- Reason:
- New required docs:
- Removed docs:
- Implementation impact:
- Review completed by:

---

## 7. One-line memory

`musubi-foundation` tells us what MUSUBI is.
`pi-musubi-core` is only allowed to make that meaning executable.
