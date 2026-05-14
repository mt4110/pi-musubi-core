# Foundation Lock

Status: Draft; aligned to accepted foundation commit `c3c0ae0`
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
- Foundation commit SHA: `c3c0ae0b3a0b9f09592b1056c7ca9997403e0830`
- Foundation commit title: `Merge pull request #130 from mt4110/feat/evaluate-post-c2-categorical-fact-non-exposure-handoff`
- Foundation PR title: `docs: evaluate post-C2 categorical fact non-exposure handoff`
- Foundation PR URL: `https://github.com/mt4110/musubi-foundation/pull/130`
- Date pinned: `2026-05-14`
- Pinned by: `Masaki Takemura`
- Pinned commit URL: `https://github.com/mt4110/musubi-foundation/commit/c3c0ae0b3a0b9f09592b1056c7ca9997403e0830`
- Previous pinned reference: `64d6348` / `Merge pull request #124 from mt4110/feat/evaluate-post-c2-non-consumption-guard-handoff`
- Post-C2 evidence source: `cfdba28` / `Merge pull request #114 from mt4110/feat/post-c2-runtime-handoff-evidence-package`
- Alignment allowance source: `69b7aa4` / `Merge pull request #116 from mt4110/feat/evaluate-post-c2-runtime-handoff-gate`
- Post-C2 implementation handoff evidence source: `ef23e88` / `Merge pull request #122 from mt4110/feat/post-c2-implementation-handoff-evidence-package`
- Post-C2 non-consumption guard handoff source: `64d6348` / `Merge pull request #124 from mt4110/feat/evaluate-post-c2-non-consumption-guard-handoff`
- Post-C2 non-consumption guard implementation closeout source: `7a2c8a0` / `Merge pull request #126 from mt4110/feat/close-post-c2-non-consumption-guard-handoff`
- Post-C2 categorical fact projection API non-exposure evidence source: `fd4465e` / `Merge pull request #128 from mt4110/feat/post-c2-categorical-fact-non-exposure-evidence`
- Post-C2 categorical fact projection API non-exposure handoff source: `c3c0ae0` / `Merge pull request #130 from mt4110/feat/evaluate-post-c2-categorical-fact-non-exposure-handoff`

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
22. `docs/adr/0019_trust_depth_mutation_registry_non_authority_boundary.md` - Status: Accepted at `6aa9922`
23. `docs/adr/0020_social_trust_writer_facts_conduct_reliability_boundary.md` - Status: Accepted at `c44f213`
24. `docs/adr/0021_relationship_depth_writer_facts_consent_mutuality_boundary.md` - Status: Accepted at `92d2775`
25. `docs/adr/0022_room_progression_relationship_depth_transition_boundary.md` - Status: Accepted at `80ab949`; runtime non-authorization clarified at `3efc33b`
26. `docs/adr/0023_durable_proof_evidence_writer_facts_boundary.md` - Status: Accepted at `496faf4`
27. `docs/adr/0024_device_attestation_proximity_proof_evidence_boundary.md` - Status: Accepted at `09f0f1d`
28. `docs/adr/0025_proof_eligibility_state_machine_boundary.md` - Status: Accepted at `346a0d6`
29. `docs/adr/0026_proof_replay_repair_trust_depth_non_authority_boundary.md` - Status: Accepted at `1295857`
30. `docs/adr/0027_server_alias_lifecycle_boundary.md` - Status: Accepted at `9c28fb7`
31. `docs/adr/0028_citadel_binding_lifecycle_boundary.md` - Status: Accepted at `a4ae40f`
32. `docs/adr/0029_authority_lease_semantics_boundary.md` - Status: Accepted at `6ae166c`
33. `docs/adr/0030_realm_relocation_lifecycle_boundary.md` - Status: Accepted at `1693f8f`
34. `docs/adr/0031_pool_attribution_quota_non_authority_boundary.md` - Status: Accepted at `759db93`
35. `docs/adr/0032_discovery_surface_constraints_boundary.md` - Status: Accepted at `e3d491f`; terminology aligned at `9c96c43`
36. `docs/adr/0033_recommendation_non_authority_boundary.md` - Status: Accepted at `92a436d`
37. `docs/adr/0034_forbidden_recommendation_signals_boundary.md` - Status: Accepted at `d8b09a8`
38. `docs/adr/0035_controlled_exceptional_account_discovery_recommendation_exclusion_boundary.md` - Status: Accepted at `7bbc87a`
39. `docs/adr/0036_recommendation_trust_depth_non_contamination_boundary.md` - Status: Accepted at `abe8c67`; replay facts clarified at `6f1150a`

### Runtime readiness layer
40. `docs/readiness/runtime_handoff_gate_criteria.md`
41. `docs/readiness/foundation_return_plan_closeout_ledger.md`
42. `docs/readiness/runtime_handoff_gate_evidence_inventory.md`
43. `docs/readiness/runtime_handoff_slice_selection_ledger.md`
44. `docs/readiness/c1_runtime_gate_invocation_guard.md`
45. `docs/readiness/c1_runtime_behavior_boundary.md`
46. `docs/readiness/c1_runtime_handoff_evidence_package.md`
47. `docs/readiness/c1_runtime_handoff_gate_decision.md`
48. `docs/readiness/c1_social_trust_intake_handoff_gate_decision.md`
49. `docs/readiness/c1_social_trust_intake_persistence_closeout_ledger.md`
50. `docs/readiness/c1_to_c2_social_trust_writer_facts_next_slice_evaluation.md`
51. `docs/readiness/c2_social_trust_source_family_gate.md`
52. `docs/readiness/c2_bounded_promise_reliability_mutation_prereqs.md`
53. `docs/readiness/c2_bounded_promise_reliability_mutation_fact_gate_readiness.md`
54. `docs/readiness/c2_bounded_promise_reliability_mutation_fact_gate.md`
55. `docs/readiness/c2_bounded_promise_reliability_foundation_lock_alignment_scope.md`
56. `docs/readiness/c2_bounded_promise_reliability_implementation_handoff_gate.md`
57. `docs/readiness/c2_bounded_promise_reliability_mutation_fact_persistence_closeout_ledger.md`
58. `docs/readiness/post_c2_next_foundation_slice_evaluation.md`
59. `docs/readiness/post_c2_runtime_handoff_evidence_package.md`
60. `docs/readiness/post_c2_runtime_handoff_gate_decision.md`
61. `docs/readiness/post_c2_foundation_lock_alignment_closeout_ledger.md`
62. `docs/readiness/post_c2_implementation_handoff_gate_decision.md`
63. `docs/readiness/post_c2_implementation_handoff_evidence_package.md`
64. `docs/readiness/post_c2_non_consumption_guard_handoff_gate_decision.md`
65. `docs/readiness/post_c2_non_consumption_guard_implementation_closeout_ledger.md`
66. `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_evidence_package.md`
67. `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_handoff_gate_decision.md`

### Detail layer
68. `docs/detail/accountability_matrix.md`
69. `docs/detail/critical_incident_and_loss.md`
70. `docs/detail/automated_decisioning_and_human_appeal.md`
71. `docs/detail/youth_safety_and_age_assurance.md`
72. `docs/detail/off_platform_handoff_and_scam_prevention.md`
73. `docs/detail/data_deletion_vs_legal_hold.md`
74. `docs/detail/realm_model.md`
75. `docs/detail/data_scope_model.md`
76. `docs/detail/mobility_model.md`
77. `docs/detail/settlement_model.md`
78. `docs/detail/settlement_backend_trait.md`
79. `docs/detail/proof_of_infrastructure.md`
80. `docs/detail/protected_groups_and_translation_safety.md`

### Whitepaper layer (contextual, not higher than detail/ADR)
81. `docs/whitepaper/01_executive_summary.md`
82. `docs/whitepaper/02_realm_model.md`
83. `docs/whitepaper/03_experience_model.md`
84. `docs/whitepaper/04_dm_shield.md`
85. `docs/whitepaper/05_trust_model.md`
86. `docs/whitepaper/06_promise_protocol.md`
87. `docs/whitepaper/07_realm_economy.md`
88. `docs/whitepaper/08_unlock_engine.md`

If any of the above are unavailable or materially inconsistent, stop and escalate.

### ADR-RC and implementation authority

`docs/adr_reconstruction/*` files remain reconstruction records only.
They are not implementation authority unless converted into an accepted foundation ADR and locked here.

`docs/adr_drafts/*` files remain draft records only.
They are not implementation authority unless converted into an accepted foundation ADR and locked here.

Accepted ADR-0006 through ADR-0036 are implementation-authorizing only within their stated scope.
ADR-0011 through ADR-0014 complete the Data Lifecycle foundation tranche for foundation scope only.
ADR-0015 through ADR-0018 complete the Account Lifecycle foundation tranche for accepted foundation scope only.
ADR-0019 through ADR-0022 complete the Trust / Depth foundation tranche for accepted foundation scope only.
ADR-0023 through ADR-0026 complete the Proof / Evidence foundation tranche for accepted foundation scope only.
ADR-0027 through ADR-0031 complete the Server / Citadel / Authority Lease / Realm Relocation / Pool foundation tranche for accepted foundation scope only.
ADR-0032 through ADR-0036 complete the Discovery / Recommendation foundation tranche for accepted foundation scope only.
Runtime implementation is not complete.
Prompt 3 implementation is not globally unblocked.
Prompt 3 implementation may proceed only where all applicable foundation ADRs and dependencies are Accepted.
The C1 Runtime Handoff Gate Decision remains accepted as a broad NO-GO record.
The C1 Social Trust Intake Handoff Gate Decision remains accepted as the historical narrow GO record for one later implementation-repo PR only.
The C1 Social Trust Intake Persistence Closeout Ledger records that the one later implementation-repo PR was consumed by `mt4110/pi-musubi-core` PR #82.
The C2 bounded Promise reliability implementation handoff gate was accepted as a narrow GO for one later implementation-repo PR only.
That one-use C2 implementation allowance was consumed by `mt4110/pi-musubi-core` PR #88 and closed out by `docs/readiness/c2_bounded_promise_reliability_mutation_fact_persistence_closeout_ledger.md`.
No remaining work may inherit permission from foundation PR #108 or implementation PR #88.
The broad runtime implementation gate result remains NO-GO.
Broad runtime implementation remains blocked.
The current narrow downstream allowance is one implementation-repo test-only PR for post-C2 categorical Social Trust fact projection API non-exposure verification.

The C2 bounded Promise reliability readiness and closeout chain is accepted for docs-only foundation semantic scope:

- `docs/readiness/c1_to_c2_social_trust_writer_facts_next_slice_evaluation.md`
- `docs/readiness/c2_social_trust_source_family_gate.md`
- `docs/readiness/c2_bounded_promise_reliability_mutation_prereqs.md`
- `docs/readiness/c2_bounded_promise_reliability_mutation_fact_gate_readiness.md`
- `docs/readiness/c2_bounded_promise_reliability_mutation_fact_gate.md`
- `docs/readiness/c2_bounded_promise_reliability_foundation_lock_alignment_scope.md`
- `docs/readiness/c2_bounded_promise_reliability_implementation_handoff_gate.md`
- `docs/readiness/c2_bounded_promise_reliability_mutation_fact_persistence_closeout_ledger.md`
- `docs/readiness/post_c2_next_foundation_slice_evaluation.md`
- `docs/readiness/post_c2_runtime_handoff_evidence_package.md`
- `docs/readiness/post_c2_runtime_handoff_gate_decision.md`
- `docs/readiness/post_c2_foundation_lock_alignment_closeout_ledger.md`
- `docs/readiness/post_c2_implementation_handoff_gate_decision.md`
- `docs/readiness/post_c2_implementation_handoff_evidence_package.md`
- `docs/readiness/post_c2_non_consumption_guard_handoff_gate_decision.md`
- `docs/readiness/post_c2_non_consumption_guard_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_handoff_gate_decision.md`

The accepted C2 gate records `bounded_promise_reliability` as the only positive Social Trust source-family candidate and accepts exact source facts and exact Social Trust mutation facts only as foundation semantic labels.
Those labels are not runtime schema names, enum values, API names, migration names, module names, or test names.
The consumed C2 implementation handoff gate authorized only categorical persistence of those accepted source and mutation facts in one later implementation-repo PR.
Accepted source and mutation fact labels must remain categorical internal writer facts only; they must not become scores, weights, ranks, display levels, public status, projection refresh, discovery / recommendation inputs, room transitions, settlement progression, Promise runtime behavior, public API, mobile UI, or Relationship Depth behavior.

Foundation PR #106 selected this repository and this file as the candidate downstream alignment scope after PR #104.
Foundation PR #108 provided the consumed narrow downstream implementation handoff authority for the C2 bounded Promise reliability persistence slice.
Foundation PR #116 provided the narrow downstream allowance for one docs-only foundation lock alignment PR in this repository, limited to `docs/foundation_lock.md`.
That allowance was consumed by `mt4110/pi-musubi-core` PR #90 and closed out by foundation PR #118.
Foundation PR #120 preserved implementation handoff NO-GO until exact slice evidence existed.
Foundation PR #122 accepted the exact candidate implementation slice as the post-C2 Social Trust categorical fact non-consumption guard.
Foundation PR #124 provided the consumed narrow downstream implementation handoff authority for one later implementation-repo PR only, limited to the post-C2 Social Trust categorical fact non-consumption guard.
That allowance was consumed by `mt4110/pi-musubi-core` PR #92 and closed out by foundation PR #126.
Foundation PR #128 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact projection API non-exposure verification.
Foundation PR #130 provides the current narrow downstream test-only handoff authority for one later implementation-repo PR only.
This update is the required lock pin for that one-use allowance.
It does not authorize implementation outside the PR #130 envelope.
It does not authorize new source facts, new mutation facts, DDL, migrations, backend runtime code, backend README updates, backend docs updates, public API changes, mobile UI, projection refresh, runtime orchestration, discovery, recommendation, room, settlement, Promise runtime, proof runtime, Relationship Depth, Social Trust scoring, public trust display, or any broader `pi-musubi-core` change.

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

The Trust / Depth, Proof / Evidence, Server / Realm / Citadel / Pool, Discovery, and Recommendation foundation tranches are FULL FOUNDATION PASS records for accepted foundation scope through ADR-0036.

This does not implement Social Trust runtime behavior, Relationship Depth runtime behavior, proof writer runtime behavior, Server alias runtime behavior, Citadel binding runtime behavior, Authority Lease runtime behavior, Realm relocation runtime behavior, Pool attribution runtime behavior, discovery runtime behavior, recommendation runtime behavior, or trust/depth contamination guards.

The C1 Runtime Handoff Gate package remains accepted as implementation-repo intake evidence only:

- `docs/readiness/c1_runtime_gate_invocation_guard.md`
- `docs/readiness/c1_runtime_behavior_boundary.md`
- `docs/readiness/c1_runtime_handoff_evidence_package.md`
- `docs/readiness/c1_runtime_handoff_gate_decision.md`

The C1 Runtime Handoff Gate Decision remains NO-GO for broad pi-musubi-core runtime implementation.

The C1 Social Trust Intake Handoff Gate Decision remains recorded as the historical narrow implementation-repo handoff decision:

- `docs/readiness/c1_social_trust_intake_handoff_gate_decision.md`

The C1 Social Trust Intake Persistence Closeout Ledger is accepted as a docs-only closeout ledger:

- `docs/readiness/c1_social_trust_intake_persistence_closeout_ledger.md`

The C1 runtime implementation gate result returned to `NO-GO` after the C1 intake allowance was consumed.
The prior C1 `NARROW GO FOR ONE LATER IMPLEMENTATION-REPO PR` was consumed by `mt4110/pi-musubi-core` PR #82.
The C1 intake persistence slice is closed.
No remaining work may inherit permission from foundation PR #92 or implementation PR #82.
The C2 bounded Promise reliability implementation handoff gate provided a separate accepted narrow GO for one later implementation-repo PR only.
That C2 implementation allowance was consumed by `mt4110/pi-musubi-core` PR #88 and is closed.
The post-C2 runtime handoff evidence package and gate decision preserved runtime NO-GO while allowing only one downstream docs-only foundation lock alignment PR in this repository.
That post-C2 lock alignment allowance was consumed by `mt4110/pi-musubi-core` PR #90 and closed out by foundation PR #118.
The post-C2 implementation handoff evidence package and handoff gate authorized one later implementation-repo PR only for the post-C2 Social Trust categorical fact non-consumption guard.
That allowance was consumed by `mt4110/pi-musubi-core` PR #92 and closed out by foundation PR #126.
The post-C2 categorical fact projection API non-exposure evidence package and handoff gate now authorize one later implementation-repo test-only PR only.
This test-only allowance is limited to `docs/foundation_lock.md` and `apps/backend/tests/post_c2_categorical_fact_projection_api_non_exposure.rs`.
It may verify that already accepted C2 categorical Social Trust facts alone do not create projection rows, public API visibility, mobile UI state, scores, weights, ranks, display levels, discovery / recommendation inputs, contact unlocks, room transitions, settlement progression, Promise runtime behavior, proof runtime behavior, or Relationship Depth behavior.
It does not authorize new source facts, new mutation facts, DDL, migrations, backend runtime code, backend README updates, backend docs updates, public API changes, mobile UI, projection refresh, runtime orchestration, discovery, recommendation, room progression, settlement behavior, Promise runtime behavior, proof runtime behavior, Relationship Depth, Social Trust scoring, public trust display, or broad runtime implementation.

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

### Current drift note
- Updated from foundation SHA: `64d634862df48d725c4a13097d52f9c0455f6cee` -> `c3c0ae0b3a0b9f09592b1056c7ca9997403e0830`
- Reason: Align implementation-repo lock with the accepted foundation state after PR #130 (`docs: evaluate post-C2 categorical fact non-exposure handoff`).
- New required docs: Post-C2 non-consumption guard implementation closeout ledger; post-C2 categorical fact projection API non-exposure evidence package; post-C2 categorical fact projection API non-exposure handoff gate decision.
- Removed docs: None.
- Implementation impact: PR #130 authorizes one later implementation-repo test-only PR. This PR may update this foundation lock and add deterministic backend integration tests proving accepted C2 categorical Social Trust facts alone do not create projection rows or public projection API visibility. New source facts, new mutation facts, DDL, migrations, backend runtime code, backend README updates, backend docs updates, public API changes, mobile UI, projection refresh, runtime orchestration, Relationship Depth, proof runtime behavior, room progression, discovery, recommendation, settlement, Promise runtime behavior, Social Trust scoring, public trust display, and paid romantic advantage remain blocked.
- Review completed by: Masaki Takemura

---

## 7. One-line memory

`musubi-foundation` tells us what MUSUBI is.
`pi-musubi-core` is only allowed to make that meaning executable.
