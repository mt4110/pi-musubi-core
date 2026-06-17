# Foundation Lock

Status: Draft; aligned to accepted foundation commit `a5a55ee`
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
- Foundation commit SHA: `a5a55ee8b86dc20b04890f302bc062301e2e1f6c`
- Foundation commit title: `Merge pull request #447 from mt4110/feat/promise-completion-state-transition-route`
- Foundation PR title: `docs: select Promise completion state transition route`
- Foundation PR URL: `https://github.com/mt4110/musubi-foundation/pull/447`
- Date pinned: `2026-06-18`
- Pinned by: `Masaki Takemura`
- Pinned commit URL: `https://github.com/mt4110/musubi-foundation/commit/a5a55ee8b86dc20b04890f302bc062301e2e1f6c`
- Previous pinned reference: `800a69c` / `Merge pull request #438 from mt4110/feat/promise-completion-writer-fact-persistence-route`
- Post-C2 evidence source: `cfdba28` / `Merge pull request #114 from mt4110/feat/post-c2-runtime-handoff-evidence-package`
- Alignment allowance source: `69b7aa4` / `Merge pull request #116 from mt4110/feat/evaluate-post-c2-runtime-handoff-gate`
- Post-C2 implementation handoff evidence source: `ef23e88` / `Merge pull request #122 from mt4110/feat/post-c2-implementation-handoff-evidence-package`
- Post-C2 non-consumption guard handoff source: `64d6348` / `Merge pull request #124 from mt4110/feat/evaluate-post-c2-non-consumption-guard-handoff`
- Post-C2 non-consumption guard implementation closeout source: `7a2c8a0` / `Merge pull request #126 from mt4110/feat/close-post-c2-non-consumption-guard-handoff`
- Post-C2 categorical fact projection API non-exposure evidence source: `fd4465e` / `Merge pull request #128 from mt4110/feat/post-c2-categorical-fact-non-exposure-evidence`
- Post-C2 categorical fact projection API non-exposure handoff source: `c3c0ae0` / `Merge pull request #130 from mt4110/feat/evaluate-post-c2-categorical-fact-non-exposure-handoff`
- Post-C2 categorical fact projection API non-exposure implementation closeout source: `f1c2bc3` / `Merge pull request #132 from mt4110/feat/close-post-c2-categorical-fact-non-exposure-handoff`
- Post-C2 categorical fact PII retention non-leakage evidence source: `1300a03` / `Merge pull request #134 from mt4110/feat/post-c2-categorical-fact-pii-retention-evidence`
- Post-C2 categorical fact PII retention non-leakage handoff source: `19ce6c6` / `Merge pull request #136 from mt4110/feat/evaluate-post-c2-categorical-fact-pii-retention-handoff`
- Post-C2 categorical fact PII retention non-leakage implementation closeout source: `3951b55` / `Merge pull request #138 from mt4110/feat/close-post-c2-categorical-fact-pii-retention-handoff`
- Post-C2 categorical fact idempotency replay evidence source: `aa2abbb` / `Merge pull request #140 from mt4110/feat/post-c2-categorical-fact-idempotency-replay-evidence`
- Post-C2 categorical fact idempotency replay handoff source: `dd283c6` / `Merge pull request #142 from mt4110/feat/evaluate-post-c2-categorical-fact-idempotency-replay-handoff`
- Post-C2 categorical fact idempotency replay implementation closeout source: `a3daf4c` / `Merge pull request #144 from mt4110/feat/close-post-c2-categorical-fact-idempotency-replay-handoff`
- Post-C2 categorical fact lifecycle replay boundary evidence source: `5446e69` / `Merge pull request #146 from mt4110/feat/post-c2-categorical-fact-lifecycle-replay-evidence`
- Post-C2 categorical fact lifecycle replay boundary handoff source: `1d12729` / `Merge pull request #148 from mt4110/feat/evaluate-post-c2-categorical-fact-lifecycle-replay-handoff`
- Post-C2 categorical fact lifecycle replay boundary implementation closeout source: `e177e10` / `Merge pull request #150 from mt4110/feat/close-post-c2-categorical-fact-lifecycle-replay-handoff`
- Post-C2 categorical fact rejection boundary non-persistence evidence source: `1e93f9f` / `Merge pull request #152 from mt4110/feat/post-c2-categorical-fact-rejection-boundary-evidence`
- Post-C2 categorical fact rejection boundary non-persistence handoff source: `37c17d2` / `Merge pull request #154 from mt4110/feat/evaluate-post-c2-categorical-fact-rejection-boundary-handoff`
- Post-C2 categorical fact rejection boundary non-persistence implementation closeout source: `4106009` / `Merge pull request #156 from mt4110/feat/close-post-c2-categorical-fact-rejection-boundary-handoff`
- Post-C2 categorical fact concurrent idempotency evidence source: `67a5ad6` / `Merge pull request #158 from mt4110/feat/post-c2-categorical-fact-concurrent-idempotency-evidence`
- Post-C2 categorical fact concurrent idempotency handoff source: `650c254` / `Merge pull request #160 from mt4110/feat/evaluate-post-c2-categorical-fact-concurrent-idempotency-handoff`
- Post-C2 categorical fact concurrent idempotency implementation closeout source: `aabbd36` / `Merge pull request #162 from mt4110/feat/close-post-c2-categorical-fact-concurrent-idempotency-handoff`
- Post-C2 categorical fact subject and Realm idempotency scope evidence source: `a1027eb` / `Merge pull request #164 from mt4110/feat/post-c2-categorical-fact-subject-realm-idempotency-evidence`
- Post-C2 categorical fact subject and Realm idempotency scope handoff source: `9910663` / `Merge pull request #166 from mt4110/feat/evaluate-post-c2-categorical-fact-subject-realm-idempotency-handoff`
- Post-C2 categorical fact subject and Realm idempotency scope implementation closeout source: `76231d7` / `Merge pull request #168 from mt4110/feat/close-post-c2-categorical-fact-subject-realm-handoff`
- Post-C2 categorical fact Controlled Exceptional Account subject evidence source: `104ce54` / `Merge pull request #170 from mt4110/feat/post-c2-controlled-exceptional-account-subject-evidence`
- Post-C2 categorical fact Controlled Exceptional Account subject handoff source: `a171f24` / `Merge pull request #173 from mt4110/feat/evaluate-post-c2-controlled-exceptional-subject-handoff`
- Post-C2 categorical fact Controlled Exceptional Account subject implementation closeout source: `28885e0` / `Merge pull request #175 from mt4110/feat/close-post-c2-controlled-exceptional-account-subject-handoff`
- Post-C2 categorical fact Controlled Exceptional Account reclassification replay evidence source: `f0b3882` / `Merge pull request #177 from mt4110/feat/post-c2-controlled-exceptional-reclassification-evidence`
- Readiness routine runner source: `5fa5bdf` / `Merge pull request #179 from mt4110/feat/readiness-routine-runner`
- Post-C2 categorical fact Controlled Exceptional Account reclassification replay handoff source: `2aced2a` / `Merge pull request #181 from mt4110/feat/post-c2-categorical-fact-controlled-exceptional-account-reclassification-replay-handoff`
- Post-C2 categorical fact Controlled Exceptional Account reclassification replay implementation closeout source: `777c4dc` / `Merge pull request #183 from mt4110/feat/close-post-c2-controlled-exceptional-account-reclassification-replay-handoff`
- Post-C2 Controlled Exceptional Account Promise participant exclusion evidence source: `dd81d7f` / `Merge pull request #185 from mt4110/feat/post-c2-controlled-exceptional-account-promise-participant-exclusion-evidence`
- RunAlways Stage 0 report-only runner source: `0a754ad` / `Merge pull request #189 from mt4110/feat/runalways-stage0-report-runner`
- Post-C2 Controlled Exceptional Account Promise participant exclusion handoff source: `f5e8576` / `Merge pull request #191 from mt4110/feat/post-c2-controlled-exceptional-account-promise-participant-exclusion-handoff`
- Post-C2 Controlled Exceptional Account Promise participant exclusion implementation closeout source: `cf2c602` / `Merge pull request #193 from mt4110/feat/close-post-c2-controlled-exceptional-account-promise-participant-exclusion-handoff`
- Post-C2 Legal Hold writer fact boundary evidence source: `016ed1d` / `Merge pull request #195 from mt4110/feat/post-c2-legal-hold-writer-fact-boundary-evidence`
- Post-C2 Legal Hold writer fact boundary handoff source: `b2938e9` / `Merge pull request #197 from mt4110/feat/post-c2-legal-hold-writer-fact-boundary-handoff`
- Post-C2 Legal Hold writer fact shape evidence source: `8de4092` / `Merge pull request #199 from mt4110/feat/post-c2-legal-hold-writer-fact-shape-evidence`
- Post-C2 Legal Hold writer fact shape handoff source: `db62383` / `Merge pull request #201 from mt4110/feat/post-c2-legal-hold-writer-fact-shape-handoff`
- Post-C2 Legal Hold writer fact downstream scope evidence source: `4359c11` / `Merge pull request #203 from mt4110/feat/post-c2-legal-hold-writer-fact-downstream-scope-evidence`
- Post-C2 Legal Hold writer fact downstream scope handoff source: `e8153a1` / `Merge pull request #205 from mt4110/feat/post-c2-legal-hold-writer-fact-downstream-scope-handoff`
- Post-C2 Legal Hold writer fact downstream scope closeout source: `d6047b8` / `Merge pull request #207 from mt4110/feat/close-post-c2-legal-hold-writer-fact-downstream-scope-handoff`
- Post-C2 Master / Submaster operator seat authority boundary evidence source: `961d09b` / `Merge pull request #208 from mt4110/feat/post-c2-master-submaster-operator-seat-authority-boundary`
- Post-C2 Master / Submaster operator seat authority boundary handoff source: `e4d5390` / `Merge pull request #209 from mt4110/feat/post-c2-master-submaster-operator-seat-authority-boundary-handoff`
- Post-C2 Master / Submaster canonical operator seat terms source: `0c5a40c` / `Merge pull request #210 from mt4110/feat/master-submaster-canonical-operator-seat-terms`
- Post-C2 Master / Submaster operator seat writer fact ADR source: `05ac25c` / `Merge pull request #211 from mt4110/feat/master-submaster-operator-seat-writer-fact-boundary`
- Post-C2 Master / Submaster operator seat writer fact lock alignment handoff source: `c2ac9a7` / `Merge pull request #212 from mt4110/feat/master-submaster-writer-fact-lock-alignment-handoff`
- Post-C2 Master / Submaster operator seat runtime implementation handoff source: `feb4ebf` / `Merge pull request #213 from mt4110/feat/master-submaster-runtime-handoff-gate`
- Post-C2 Master / Submaster operator seat runtime non-authority implementation closeout source: `fab636f` / `Merge pull request #214 from mt4110/feat/close-master-submaster-runtime-non-authority-handoff`
- Post-C2 Master / Submaster active authority writer fact shape evidence source: `7f83d33` / `Merge pull request #216 from mt4110/feat/master-submaster-active-authority-fact-shape`
- Post-C2 legal privacy consumer-contract quantitative gate evidence source: `7f83d33` / `Merge pull request #216 from mt4110/feat/master-submaster-active-authority-fact-shape`
- Post-C2 Master / Submaster active authority writer fact shape handoff source: `0aa667b` / `Merge pull request #217 from mt4110/feat/master-submaster-active-authority-handoff-gate`
- Post-C2 legal privacy consumer-contract quantitative gate handoff source: `6dbf34b` / `Merge pull request #218 from mt4110/feat/legal-privacy-quantitative-gate-handoff`
- RunAlways Lane Controller v0.1 operations protocol source: `9dffc01` / `Merge pull request #225 from mt4110/feat/runalways-lane-controller-v0`
- Post-C2 orchestration command lease reclaim test handoff source: `575b60c` / `Merge pull request #221 from mt4110/feat/orchestration-command-lease-reclaim-handoff`
- Post-C2 orchestration command lease reclaim test implementation closeout source: `2821fc1` / `Merge pull request #223 from mt4110/feat/close-orchestration-command-lease-reclaim-handoff`
- Post-C2 orchestration prune nonterminal preservation evidence source: `23cecc1` / `Merge pull request #227 from mt4110/feat/orchestration-prune-nonterminal-evidence`
- Post-C2 orchestration prune nonterminal preservation handoff source: `e619312` / `Merge pull request #229 from mt4110/feat/post-c2-orchestration-prune-nonterminal-preservation-handoff`
- Post-C2 orchestration prune nonterminal preservation implementation closeout source: `7713770` / `Merge pull request #231 from mt4110/feat/close-orchestration-prune-nonterminal-preservation-handoff`
- Post-C2 orchestration terminal archive payload preservation evidence source: `b8ee362` / `Merge pull request #233 from mt4110/feat/orchestration-terminal-archive-payload-evidence`
- Post-C2 orchestration terminal archive payload preservation handoff source: `b3e8c9b` / `Merge pull request #235 from mt4110/feat/post-c2-orchestration-terminal-archive-payload-preservation-handoff`
- Post-C2 orchestration terminal archive payload preservation implementation closeout source: `ebccabd` / `Merge pull request #237 from mt4110/feat/close-orchestration-terminal-archive-payload-handoff`
- Post-C2 orchestration terminal quarantine archive diagnostics preservation evidence source: `393f370` / `Merge pull request #239 from mt4110/feat/orchestration-terminal-quarantine-archive-diagnostics-evidence`
- Post-C2 orchestration terminal quarantine archive diagnostics preservation handoff source: `5862e40` / `Merge pull request #241 from mt4110/feat/post-c2-orchestration-terminal-quarantine-archive-diagnostics-preservation-handoff`
- Post-C2 orchestration terminal quarantine archive diagnostics preservation implementation closeout source: `6d72b8d` / `Merge pull request #243 from mt4110/feat/close-orchestration-terminal-quarantine-archive-diagnostics-handoff`
- Post-C2 orchestration prune archive conflict idempotency evidence source: `57ee247` / `Merge pull request #245 from mt4110/feat/orchestration-prune-archive-conflict-idempotency-evidence`
- Post-C2 orchestration prune archive conflict idempotency handoff source: `4f1faaf` / `Merge pull request #247 from mt4110/feat/post-c2-orchestration-prune-archive-conflict-idempotency-handoff`
- Post-C2 orchestration prune archive conflict idempotency implementation closeout source: `0e2787d` / `Merge pull request #249 from mt4110/feat/close-orchestration-prune-archive-conflict-idempotency-handoff`
- Post-C2 orchestration terminal prune retention eligibility evidence source: `62f367f` / `Merge pull request #251 from mt4110/feat/orchestration-terminal-prune-retention-eligibility-evidence`
- Post-C2 orchestration terminal prune retention eligibility handoff source: `a77f78e` / `Merge pull request #253 from mt4110/feat/post-c2-orchestration-terminal-prune-retention-eligibility-handoff`
- Post-C2 orchestration terminal prune retention eligibility implementation closeout source: `be7d8bb` / `docs: close orchestration terminal prune retention eligibility handoff`
- Post-C2 orchestration prune mixed eligibility separation evidence source: `ab8f6f5` / `docs: define orchestration prune mixed eligibility separation evidence`
- Post-C2 orchestration prune mixed eligibility separation handoff source: `52d20d0` / `docs: add orchestration prune mixed eligibility separation handoff gate`
- Post-C2 orchestration prune mixed eligibility separation implementation closeout source: `7d732d9` / `docs: close orchestration prune mixed eligibility separation handoff`
- Post-C2 orchestration prune deterministic outcome ordering evidence source: `4d95282` / `Merge pull request #263 from mt4110/feat/orchestration-prune-deterministic-outcome-ordering-evidence`
- Post-C2 orchestration prune deterministic outcome ordering handoff source: `985eb58` / `Merge pull request #265 from mt4110/feat/post-c2-orchestration-prune-deterministic-outcome-ordering-handoff`
- Post-C2 orchestration prune deterministic outcome ordering implementation closeout source: `b98fc53` / `Merge pull request #267 from mt4110/feat/close-orchestration-prune-deterministic-outcome-ordering-handoff`
- Post-C2 orchestration prune outbox attempt archive completeness evidence source: `97aa9e0` / `Merge pull request #269 from mt4110/feat/orchestration-prune-outbox-attempt-archive-completeness-evidence`
- Post-C2 orchestration prune outbox attempt archive completeness handoff source: `c9b6e37` / `Merge pull request #271 from mt4110/feat/post-c2-orchestration-prune-outbox-attempt-archive-completeness-handoff`
- Post-C2 orchestration prune outbox attempt archive completeness implementation closeout source: `f314d55` / `Merge pull request #273 from mt4110/feat/close-orchestration-prune-outbox-attempt-archive-completeness-handoff`
- Post-C2 orchestration prune command inbox archive completeness evidence source: `963f0e6` / `Merge pull request #275 from mt4110/feat/orchestration-prune-command-inbox-archive-completeness-evidence`
- Post-C2 orchestration prune command inbox archive completeness handoff source: `c2729ce` / `Merge pull request #277 from mt4110/feat/post-c2-orchestration-prune-command-inbox-archive-completeness-handoff`
- Post-C2 orchestration prune command inbox archive completeness implementation closeout source: `5ea1143` / `Merge pull request #279 from mt4110/feat/close-orchestration-prune-command-inbox-archive-completeness-handoff`
- Post-C2 orchestration prune command inbox archive completeness formatting compliance evidence source: `daf0cab` / `Merge pull request #281 from mt4110/feat/orchestration-prune-command-inbox-formatting-compliance-evidence`
- Post-C2 orchestration prune command inbox archive completeness formatting compliance handoff source: `17ee7d9` / `Merge pull request #283 from mt4110/feat/post-c2-orchestration-prune-command-inbox-formatting-compliance-handoff`
- Post-C2 orchestration prune command inbox archive completeness formatting compliance implementation closeout source: `6a20749` / `Merge pull request #285 from mt4110/feat/close-command-inbox-archive-formatting-compliance-handoff`
- Post-C2 orchestration prune archive conflict mismatch fail-closed evidence source: `19a9d2a` / `Merge pull request #287 from mt4110/feat/orchestration-prune-archive-conflict-mismatch-evidence`
- Post-C2 orchestration prune archive conflict mismatch fail-closed handoff source: `6de5734` / `Merge pull request #289 from mt4110/feat/post-c2-orchestration-prune-archive-conflict-mismatch-fail-closed-handoff`
- Post-C2 orchestration prune archive conflict mismatch corrective evidence source: `59ed92b` / `Merge pull request #291 from mt4110/feat/orchestration-prune-archive-conflict-mismatch-corrective-evidence`
- Post-C2 orchestration prune archive conflict mismatch corrective handoff source: `5ceba0d` / `Merge pull request #293 from mt4110/feat/post-c2-orchestration-prune-archive-conflict-mismatch-corrective-handoff`
- Post-C2 orchestration prune archive conflict mismatch corrective implementation closeout source: `2bf19af` / `Merge pull request #295 from mt4110/feat/close-orchestration-prune-archive-conflict-mismatch-corrective-handoff`
- Post-C2 orchestration prune archive conflict mismatch side-effect containment evidence source: `14f26c4` / `Merge pull request #297 from mt4110/feat/orchestration-prune-archive-conflict-mismatch-side-effect-containment-evidence`
- Post-C2 orchestration prune archive conflict mismatch side-effect containment handoff source: `1e3a8ae` / `Merge pull request #299 from mt4110/feat/post-c2-orchestration-prune-archive-conflict-mismatch-side-effect-containment-handoff`
- Post-C2 orchestration prune archive conflict mismatch side-effect containment corrective evidence source: `2b73582` / `Merge pull request #301 from mt4110/feat/orchestration-prune-archive-conflict-mismatch-side-effect-corrective-evidence`
- Post-C2 orchestration prune archive conflict mismatch side-effect containment corrective handoff source: `cac6b16` / `Merge pull request #303 from mt4110/feat/post-c2-orchestration-prune-archive-conflict-mismatch-side-effect-corrective-handoff`
- Post-C2 orchestration prune archive conflict mismatch side-effect containment corrective implementation closeout source: `e793005` / `Merge pull request #305 from mt4110/feat/close-orchestration-prune-archive-conflict-mismatch-side-effect-corrective-handoff`
- C1 Trust / Depth / Proof authority evidence source: `a05577d` / `Merge pull request #326 from mt4110/feat/c1-trust-depth-proof-authority-evidence`
- C1 Trust / Depth / Proof authority test-only handoff source: `d54a644` / `Merge pull request #327 from mt4110/feat/c1-trust-depth-proof-test-only-handoff`
- C1 first positive source scope decision source: `0f926f9` / `Merge pull request #346 from mt4110/feat/c1-first-positive-source-scope`
- Core implementation authority map and C1 Social Trust positive source implementation handoff source: `bf7cde8` / `Merge pull request #348 from mt4110/feat/core-authority-map-c1-social-trust-handoff`
- C1 Social Trust positive source implementation closeout source: `e9b785e` / `Merge pull request #350 from mt4110/feat/close-c1-social-trust-positive-source-handoff`
- C1 Social Trust writer record persistence preflight no-op closeout source: `7febf11` / `Merge pull request #358 from mt4110/feat/c1-social-trust-writer-record-no-op-closeout`
- Promise Realm projection participant surface authority detail record source: `eb90be8` / `Merge pull request #364 from mt4110/feat/participant-surface-authority-sufficiency`
- Promise completion proof eligibility authority detail record source: `ac20cf6` / `Merge pull request #366 from mt4110/feat/promise-completion-proof-eligibility-authority`
- Promise completion source taxonomy active founder answer and detail record source: `9cc2d63` / `Merge pull request #370 from mt4110/feat/promise-completion-source-taxonomy-answer`
- Promise completion state machine authority source: `fb73d1a` / `Merge pull request #372 from mt4110/feat/promise-completion-state-machine-authority`
- Promise completion writer fact record family authority source: `9c9b741` / `Merge pull request #374 from mt4110/feat/promise-completion-writer-fact-record-family-authority`
- Promise completion implementation gate readiness gap ledger source: `a75c66c` / `Merge pull request #376 from mt4110/feat/promise-completion-implementation-gate-readiness-gaps`
- Promise completion downstream gate decision packet source: `810e1c4` / `Merge pull request #378 from mt4110/feat/promise-completion-downstream-gate-decision-packet`
- Promise completion implementation-ready design envelope source: `8d1ee8e` / `Merge pull request #380 from mt4110/feat/promise-completion-implementation-ready-design-envelope`
- Promise completion downstream gate sufficiency decision source: `48232bc` / `Merge pull request #382 from mt4110/feat/promise-completion-downstream-gate-sufficiency`
- Promise completion downstream gate selection decision source: `5f68437` / `Merge pull request #384 from mt4110/feat/promise-completion-downstream-gate-selection`
- Promise completion foundation lock alignment closeout source: `eeb38fc` / `Merge pull request #386 from mt4110/feat/promise-completion-lock-alignment-closeout`
- Promise completion post-closeout route selection source: `fcfa7a8` / `Merge pull request #388 from mt4110/feat/promise-completion-post-closeout-route-selection`
- Promise completion persistence preflight design packet source: `f95cd94` / `Merge pull request #390 from mt4110/feat/promise-completion-persistence-preflight-packet`
- Promise completion persistence preflight sufficiency decision source: `7203c2b` / `Merge pull request #392 from mt4110/feat/promise-completion-persistence-preflight-sufficiency`
- Promise completion post-sufficiency route selection source: `d69c257` / `Merge pull request #394 from mt4110/feat/promise-completion-post-sufficiency-route-selection`
- Promise completion persistence authority detail record source: `6600863` / `Merge pull request #396 from mt4110/feat/promise-completion-persistence-authority`
- Promise completion persistence authority sufficiency decision source: `06eda80` / `Merge pull request #398 from mt4110/feat/promise-completion-persistence-authority-sufficiency`
- Promise completion post-persistence-authority route selection source: `ce89d9c` / `Merge pull request #400 from mt4110/feat/promise-completion-post-persistence-authority-route-selection`
- Promise completion preflight route decision packet source: `dacddab` / `Merge pull request #402 from mt4110/feat/promise-completion-preflight-route-decision-packet`
- Promise completion preflight route packet sufficiency decision source: `f9beabc` / `Merge pull request #404 from mt4110/feat/promise-completion-preflight-route-packet-sufficiency`
- Promise completion preflight route selection source: `0e33cea` / `Merge pull request #406 from mt4110/feat/promise-completion-preflight-route-selection`
- Promise completion writer fact persistence preflight source: `00f88bf` / `Merge pull request #408 from mt4110/feat/promise-completion-writer-fact-persistence-preflight`
- Promise completion writer fact persistence preflight sufficiency decision source: `6c52333` / `Merge pull request #410 from mt4110/feat/promise-completion-persistence-preflight-sufficiency-v2`
- Promise completion post-preflight route selection source: `d3dbac2` / `Merge pull request #412 from mt4110/feat/promise-completion-post-preflight-route-selection`
- Promise completion narrow handoff decision packet source: `62dfc07` / `Merge pull request #414 from mt4110/feat/promise-completion-narrow-handoff-decision-packet`
- Promise completion narrow handoff packet sufficiency decision source: `a000b1a` / `Merge pull request #416 from mt4110/feat/promise-completion-narrow-handoff-packet-sufficiency`
- Promise completion post-narrow-handoff route selection source: `ea50bac` / `Merge pull request #418 from mt4110/feat/promise-completion-post-narrow-handoff-route-selection`
- Promise completion core touch precondition matrix source: `2da36cd` / `Merge pull request #420 from mt4110/feat/promise-completion-core-touch-preconditions`
- Promise completion core touch precondition matrix sufficiency decision source: `f766909` / `Merge pull request #422 from mt4110/feat/promise-completion-core-touch-precondition-sufficiency`
- Promise completion post-precondition route selection source: `e29c45d` / `Merge pull request #424 from mt4110/feat/promise-completion-post-precondition-route`
- Promise completion test-only hard-exclusion decision packet source: `d8f6ece` / `Merge pull request #426 from mt4110/feat/promise-completion-test-only-exclusion-packet`
- Promise completion test-only hard-exclusion packet sufficiency decision source: `4840779` / `Merge pull request #428 from mt4110/feat/promise-completion-test-only-exclusion-sufficiency`
- Promise completion post-test-only-exclusion route selection source: `81e127b` / `Merge pull request #430 from mt4110/feat/promise-completion-post-test-exclusion-route`
- Promise completion post-test-only-exclusion foundation lock alignment closeout source: `09c53e7` / `Merge pull request #432 from mt4110/feat/promise-completion-post-test-exclusion-lock-closeout`
- Promise completion test-only hard-exclusion route selection source: `9e500b2` / `Merge pull request #434 from mt4110/feat/promise-completion-test-only-hard-exclusion-route`
- Promise completion test-only hard-exclusion closeout source: `69488af` / `Merge pull request #436 from mt4110/feat/promise-completion-hard-exclusion-closeout`
- Promise completion narrow writer fact persistence route selection source: `800a69c` / `Merge pull request #438 from mt4110/feat/promise-completion-writer-fact-persistence-route`
- Promise completion narrow writer fact persistence closeout source: `fbc68a8` / `Merge pull request #439 from mt4110/feat/promise-completion-writer-fact-persistence-closeout`
- Promise completion post-writer-fact-persistence route selection source: `89b4228` / `Merge pull request #441 from mt4110/feat/promise-completion-post-writer-fact-persistence-route`
- Promise completion state transition runtime preflight packet source: `fbca981` / `Merge pull request #443 from mt4110/feat/promise-completion-state-transition-preflight-packet`
- Promise completion state transition runtime preflight packet sufficiency decision source: `3fd5294` / `Merge pull request #445 from mt4110/feat/promise-completion-state-transition-preflight-sufficiency`
- Promise completion narrow state transition runtime route selection source: `a5a55ee` / `Merge pull request #447 from mt4110/feat/promise-completion-state-transition-route`

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
40. `docs/adr/0037_master_submaster_operator_seat_writer_fact_boundary.md` - Status: Accepted at `05ac25c`

### Runtime readiness layer
41. `docs/readiness/runtime_handoff_gate_criteria.md`
42. `docs/readiness/foundation_return_plan_closeout_ledger.md`
43. `docs/readiness/runtime_handoff_gate_evidence_inventory.md`
44. `docs/readiness/runtime_handoff_slice_selection_ledger.md`
45. `docs/readiness/c1_runtime_gate_invocation_guard.md`
46. `docs/readiness/c1_runtime_behavior_boundary.md`
47. `docs/readiness/c1_runtime_handoff_evidence_package.md`
48. `docs/readiness/c1_runtime_handoff_gate_decision.md`
49. `docs/readiness/c1_social_trust_intake_handoff_gate_decision.md`
50. `docs/readiness/c1_social_trust_intake_persistence_closeout_ledger.md`
51. `docs/readiness/c1_to_c2_social_trust_writer_facts_next_slice_evaluation.md`
52. `docs/readiness/c2_social_trust_source_family_gate.md`
53. `docs/readiness/c2_bounded_promise_reliability_mutation_prereqs.md`
54. `docs/readiness/c2_bounded_promise_reliability_mutation_fact_gate_readiness.md`
55. `docs/readiness/c2_bounded_promise_reliability_mutation_fact_gate.md`
56. `docs/readiness/c2_bounded_promise_reliability_foundation_lock_alignment_scope.md`
57. `docs/readiness/c2_bounded_promise_reliability_implementation_handoff_gate.md`
58. `docs/readiness/c2_bounded_promise_reliability_mutation_fact_persistence_closeout_ledger.md`
59. `docs/readiness/post_c2_next_foundation_slice_evaluation.md`
60. `docs/readiness/post_c2_runtime_handoff_evidence_package.md`
61. `docs/readiness/post_c2_runtime_handoff_gate_decision.md`
62. `docs/readiness/post_c2_foundation_lock_alignment_closeout_ledger.md`
63. `docs/readiness/post_c2_implementation_handoff_gate_decision.md`
64. `docs/readiness/post_c2_implementation_handoff_evidence_package.md`
65. `docs/readiness/post_c2_non_consumption_guard_handoff_gate_decision.md`
66. `docs/readiness/post_c2_non_consumption_guard_implementation_closeout_ledger.md`
67. `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_evidence_package.md`
68. `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_handoff_gate_decision.md`
69. `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_implementation_closeout_ledger.md`
70. `docs/readiness/post_c2_categorical_fact_pii_retention_non_leakage_evidence_package.md`
71. `docs/readiness/post_c2_categorical_fact_pii_retention_non_leakage_handoff_gate_decision.md`
72. `docs/readiness/post_c2_categorical_fact_pii_retention_non_leakage_implementation_closeout_ledger.md`
73. `docs/readiness/post_c2_categorical_fact_idempotency_replay_evidence_package.md`
74. `docs/readiness/post_c2_categorical_fact_idempotency_replay_handoff_gate_decision.md`
75. `docs/readiness/post_c2_categorical_fact_idempotency_replay_implementation_closeout_ledger.md`
76. `docs/readiness/post_c2_categorical_fact_lifecycle_replay_boundary_evidence_package.md`
77. `docs/readiness/post_c2_categorical_fact_lifecycle_replay_boundary_handoff_gate_decision.md`
78. `docs/readiness/post_c2_categorical_fact_lifecycle_replay_boundary_implementation_closeout_ledger.md`
79. `docs/readiness/post_c2_categorical_fact_rejection_boundary_non_persistence_evidence_package.md`
80. `docs/readiness/post_c2_categorical_fact_rejection_boundary_non_persistence_handoff_gate_decision.md`
81. `docs/readiness/post_c2_categorical_fact_rejection_boundary_non_persistence_implementation_closeout_ledger.md`
82. `docs/readiness/post_c2_categorical_fact_concurrent_idempotency_evidence_package.md`
83. `docs/readiness/post_c2_categorical_fact_concurrent_idempotency_handoff_gate_decision.md`
84. `docs/readiness/post_c2_categorical_fact_concurrent_idempotency_implementation_closeout_ledger.md`
85. `docs/readiness/post_c2_categorical_fact_subject_realm_idempotency_scope_evidence_package.md`
86. `docs/readiness/post_c2_categorical_fact_subject_realm_idempotency_scope_handoff_gate_decision.md`
87. `docs/readiness/post_c2_categorical_fact_subject_realm_idempotency_scope_implementation_closeout_ledger.md`
88. `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_subject_evidence_package.md`
89. `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_subject_handoff_gate_decision.md`
90. `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_subject_implementation_closeout_ledger.md`
91. `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_reclassification_replay_evidence_package.md`
92. `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_reclassification_replay_handoff_gate_decision.md`
93. `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_reclassification_replay_implementation_closeout_ledger.md`
94. `docs/readiness/post_c2_controlled_exceptional_account_promise_participant_exclusion_evidence_package.md`
95. `docs/readiness/post_c2_controlled_exceptional_account_promise_participant_exclusion_handoff_gate_decision.md`
96. `docs/readiness/post_c2_controlled_exceptional_account_promise_participant_exclusion_implementation_closeout_ledger.md`
97. `docs/readiness/post_c2_legal_hold_writer_fact_boundary_evidence_package.md`
98. `docs/readiness/post_c2_legal_hold_writer_fact_boundary_handoff_gate_decision.md`
99. `docs/readiness/post_c2_legal_hold_writer_fact_shape_evidence_package.md`
100. `docs/readiness/post_c2_legal_hold_writer_fact_shape_handoff_gate_decision.md`
101. `docs/readiness/post_c2_legal_hold_writer_fact_downstream_scope_evidence_package.md`
102. `docs/readiness/post_c2_legal_hold_writer_fact_downstream_scope_handoff_gate_decision.md`
103. `docs/readiness/post_c2_master_submaster_operator_seat_writer_fact_lock_alignment_handoff_gate_decision.md`
104. `docs/readiness/post_c2_master_submaster_operator_seat_runtime_implementation_handoff_gate_decision.md`
105. `docs/readiness/post_c2_master_submaster_operator_seat_runtime_non_authority_implementation_closeout_ledger.md`
106. `docs/readiness/post_c2_master_submaster_operator_seat_active_authority_writer_fact_shape_evidence_package.md`
107. `docs/readiness/post_c2_legal_privacy_consumer_contract_quantitative_gate_evidence_package.md`
108. `docs/readiness/post_c2_master_submaster_operator_seat_active_authority_writer_fact_shape_handoff_gate_decision.md`
109. `docs/readiness/post_c2_legal_privacy_consumer_contract_quantitative_gate_handoff_gate_decision.md`
110. `docs/readiness/post_c2_orchestration_command_lease_reclaim_test_handoff_gate_decision.md`
111. `docs/readiness/post_c2_orchestration_command_lease_reclaim_test_implementation_closeout_ledger.md`
112. `docs/readiness/post_c2_orchestration_prune_nonterminal_preservation_evidence_package.md`
113. `docs/readiness/post_c2_orchestration_prune_nonterminal_preservation_handoff_gate_decision.md`
114. `docs/readiness/post_c2_orchestration_prune_nonterminal_preservation_implementation_closeout_ledger.md`
115. `docs/readiness/post_c2_orchestration_terminal_archive_payload_preservation_evidence_package.md`
116. `docs/readiness/post_c2_orchestration_terminal_archive_payload_preservation_handoff_gate_decision.md`
117. `docs/readiness/post_c2_orchestration_terminal_archive_payload_preservation_implementation_closeout_ledger.md`
118. `docs/readiness/post_c2_orchestration_terminal_quarantine_archive_diagnostics_preservation_evidence_package.md`
119. `docs/readiness/post_c2_orchestration_terminal_quarantine_archive_diagnostics_preservation_handoff_gate_decision.md`
120. `docs/readiness/post_c2_orchestration_terminal_quarantine_archive_diagnostics_preservation_implementation_closeout_ledger.md`
121. `docs/readiness/post_c2_orchestration_prune_archive_conflict_idempotency_evidence_package.md`
122. `docs/readiness/post_c2_orchestration_prune_archive_conflict_idempotency_handoff_gate_decision.md`
123. `docs/readiness/post_c2_orchestration_prune_archive_conflict_idempotency_implementation_closeout_ledger.md`
124. `docs/readiness/post_c2_orchestration_terminal_prune_retention_eligibility_evidence_package.md`
125. `docs/readiness/post_c2_orchestration_terminal_prune_retention_eligibility_handoff_gate_decision.md`
126. `docs/readiness/post_c2_orchestration_terminal_prune_retention_eligibility_implementation_closeout_ledger.md`
127. `docs/readiness/post_c2_orchestration_prune_mixed_eligibility_separation_evidence_package.md`
128. `docs/readiness/post_c2_orchestration_prune_mixed_eligibility_separation_handoff_gate_decision.md`
129. `docs/readiness/post_c2_orchestration_prune_mixed_eligibility_separation_implementation_closeout_ledger.md`
130. `docs/readiness/post_c2_orchestration_prune_deterministic_outcome_ordering_evidence_package.md`
131. `docs/readiness/post_c2_orchestration_prune_deterministic_outcome_ordering_handoff_gate_decision.md`
132. `docs/readiness/post_c2_orchestration_prune_deterministic_outcome_ordering_implementation_closeout_ledger.md`
133. `docs/readiness/post_c2_orchestration_prune_outbox_attempt_archive_completeness_evidence_package.md`
134. `docs/readiness/post_c2_orchestration_prune_outbox_attempt_archive_completeness_handoff_gate_decision.md`
135. `docs/readiness/post_c2_orchestration_prune_outbox_attempt_archive_completeness_implementation_closeout_ledger.md`
136. `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_evidence_package.md`
137. `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_handoff_gate_decision.md`
138. `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_implementation_closeout_ledger.md`
139. `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_formatting_compliance_evidence_package.md`
140. `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_formatting_compliance_handoff_gate_decision.md`
141. `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_formatting_compliance_implementation_closeout_ledger.md`
142. `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_fail_closed_evidence_package.md`
143. `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_fail_closed_handoff_gate_decision.md`
144. `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_corrective_evidence_package.md`
145. `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_corrective_handoff_gate_decision.md`
146. `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_side_effect_containment_corrective_implementation_closeout_ledger.md`
147. `docs/readiness/c1_trust_depth_proof_authority_foundation_evidence_package.md`
148. `docs/readiness/c1_trust_depth_proof_authority_source_law_ledger.md`
149. `docs/readiness/c1_trust_depth_proof_authority_non_authorization_guard.md`
150. `docs/readiness/c1_trust_depth_proof_authority_test_only_handoff_gate_decision.md`
151. `docs/readiness/c1_first_positive_source_scope_decision.md`
152. `docs/readiness/core_implementation_authority_map.md`
153. `docs/readiness/c1_social_trust_positive_source_implementation_handoff_gate_decision.md`
154. `docs/readiness/c1_social_trust_positive_source_implementation_closeout_ledger.md`
155. `docs/readiness/c1_social_trust_writer_record_persistence_preflight_handoff_no_op_closeout_ledger.md`
156. `docs/readiness/promise_realm_projection_participant_surface_authority_detail_record.md`
157. `docs/readiness/promise_completion_proof_eligibility_authority_detail_record.md`
158. `docs/readiness/promise_completion_source_taxonomy_active_founder_answer_ledger.md`
159. `docs/readiness/promise_completion_source_taxonomy_detail_record.md`
160. `docs/readiness/promise_completion_state_machine_authority_detail_record.md`
161. `docs/readiness/promise_completion_writer_fact_record_family_authority_detail_record.md`
162. `docs/readiness/promise_completion_implementation_gate_readiness_gap_ledger.md`
163. `docs/readiness/promise_completion_downstream_gate_decision_packet.md`
164. `docs/readiness/promise_completion_implementation_ready_design_envelope.md`
165. `docs/readiness/promise_completion_downstream_gate_sufficiency_decision.md`
166. `docs/readiness/promise_completion_downstream_gate_selection_decision.md`
167. `docs/readiness/promise_completion_foundation_lock_alignment_closeout_ledger.md`
168. `docs/readiness/promise_completion_post_closeout_next_downstream_route_selection.md`
169. `docs/readiness/promise_completion_persistence_preflight_design_packet.md`
170. `docs/readiness/promise_completion_persistence_preflight_sufficiency_decision.md`
171. `docs/readiness/promise_completion_post_sufficiency_next_route_selection.md`
172. `docs/readiness/promise_completion_persistence_authority_detail_record.md`
173. `docs/readiness/promise_completion_persistence_authority_sufficiency_decision.md`
174. `docs/readiness/promise_completion_post_persistence_authority_route_selection.md`
175. `docs/readiness/promise_completion_preflight_route_decision_packet.md`
176. `docs/readiness/promise_completion_preflight_route_packet_sufficiency_decision.md`
177. `docs/readiness/promise_completion_preflight_route_selection_decision.md`
178. `docs/readiness/promise_completion_writer_fact_persistence_representability_preflight.md`
179. `docs/readiness/promise_completion_writer_fact_persistence_preflight_sufficiency_decision.md`
180. `docs/readiness/promise_completion_post_preflight_route_selection.md`
181. `docs/readiness/promise_completion_narrow_handoff_decision_packet.md`
182. `docs/readiness/promise_completion_narrow_handoff_packet_sufficiency_decision.md`
183. `docs/readiness/promise_completion_post_narrow_handoff_route_selection.md`
184. `docs/readiness/promise_completion_core_touch_precondition_matrix.md`
185. `docs/readiness/promise_completion_core_touch_precondition_matrix_sufficiency_decision.md`
186. `docs/readiness/promise_completion_post_precondition_route_selection.md`
187. `docs/readiness/promise_completion_test_only_hard_exclusion_decision_packet.md`
188. `docs/readiness/promise_completion_test_only_hard_exclusion_packet_sufficiency_decision.md`
189. `docs/readiness/promise_completion_post_test_only_exclusion_sufficiency_route_selection.md`
190. `docs/readiness/promise_completion_post_test_only_exclusion_foundation_lock_alignment_closeout_ledger.md`
191. `docs/readiness/promise_completion_post_lock_alignment_test_only_hard_exclusion_route_selection.md`
192. `docs/readiness/promise_completion_test_only_hard_exclusion_closeout_ledger.md`
193. `docs/readiness/promise_completion_narrow_writer_fact_persistence_route_selection.md`
194. `docs/readiness/promise_completion_narrow_writer_fact_persistence_closeout_ledger.md`
195. `docs/readiness/promise_completion_post_writer_fact_persistence_closeout_route_selection.md`
196. `docs/readiness/promise_completion_state_transition_runtime_preflight_decision_packet.md`
197. `docs/readiness/promise_completion_state_transition_runtime_preflight_packet_sufficiency_decision.md`
198. `docs/readiness/promise_completion_post_state_transition_preflight_sufficiency_route_selection.md`

### Operations layer
199. `docs/operations/readiness_routine.md`
200. `docs/operations/runalways_readiness_orchestrator_design.md`
201. `docs/operations/runalways_readiness_orchestrator_stage0.md`
202. `docs/operations/runalways_lane_controller_v0_1.md`

### Detail layer
203. `docs/detail/accountability_matrix.md`
204. `docs/detail/critical_incident_and_loss.md`
205. `docs/detail/automated_decisioning_and_human_appeal.md`
206. `docs/detail/youth_safety_and_age_assurance.md`
207. `docs/detail/off_platform_handoff_and_scam_prevention.md`
208. `docs/detail/data_deletion_vs_legal_hold.md`
209. `docs/detail/security_and_autonomy_hardening.md`
210. `docs/detail/realm_model.md`
211. `docs/detail/data_scope_model.md`
212. `docs/detail/mobility_model.md`
213. `docs/detail/settlement_model.md`
214. `docs/detail/settlement_backend_trait.md`
215. `docs/detail/proof_of_infrastructure.md`
216. `docs/detail/protected_groups_and_translation_safety.md`

### Whitepaper layer (contextual, not higher than detail/ADR)
217. `docs/whitepaper/01_executive_summary.md`
218. `docs/whitepaper/02_realm_model.md`
219. `docs/whitepaper/03_experience_model.md`
220. `docs/whitepaper/04_dm_shield.md`
221. `docs/whitepaper/05_trust_model.md`
222. `docs/whitepaper/06_promise_protocol.md`
223. `docs/whitepaper/07_realm_economy.md`
224. `docs/whitepaper/08_unlock_engine.md`

If any of the above are unavailable or materially inconsistent, stop and escalate.

### ADR-RC and implementation authority

`docs/adr_reconstruction/*` files remain reconstruction records only.
They are not implementation authority unless converted into an accepted foundation ADR and locked here.

`docs/adr_drafts/*` files remain draft records only.
They are not implementation authority unless converted into an accepted foundation ADR and locked here.

Accepted ADR-0006 through ADR-0037 are implementation-authorizing only within their stated scope.
ADR-0011 through ADR-0014 complete the Data Lifecycle foundation tranche for foundation scope only.
ADR-0015 through ADR-0018 complete the Account Lifecycle foundation tranche for accepted foundation scope only.
ADR-0019 through ADR-0022 complete the Trust / Depth foundation tranche for accepted foundation scope only.
ADR-0023 through ADR-0026 complete the Proof / Evidence foundation tranche for accepted foundation scope only.
ADR-0027 through ADR-0031 complete the Server / Citadel / Authority Lease / Realm Relocation / Pool foundation tranche for accepted foundation scope only.
ADR-0032 through ADR-0036 complete the Discovery / Recommendation foundation tranche for accepted foundation scope only.
ADR-0037 accepts the Master / Submaster operator-seat writer fact boundary for accepted foundation scope only.
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
No current narrow downstream runtime allowance remains after the post-C2 orchestration prune archive conflict mismatch side-effect containment corrective implementation closeout.

Foundation PR #326 accepted C1 Trust / Depth / Proof authority evidence only.
It did not decide exact Social Trust source fact taxonomy, exact Relationship Depth source fact taxonomy, proof eligibility semantics, mutation weights, recovery ceilings, scoring, display, public trust posture, provider guarantees, or downstream runtime behavior.
Foundation PR #327 provided the prior one-use downstream test-only handoff authority for C1 Trust / Depth / Proof hard non-authority tests.
That allowance was consumed by `mt4110/pi-musubi-core` PR #158.
No remaining work may inherit permission from foundation PR #327 or implementation PR #158.
Foundation PR #346 recorded the C1 first positive source scope as Social Trust only: fulfilled commitments / Promise follow-through plus accountable completion behavior.
Foundation PR #346 did not authorize foundation lock alignment, downstream work, runtime implementation, runtime tests, DDL, migrations, schema-only work, public API, mobile UI, projection, scoring, display, Relationship Depth mutation, proof runtime, or `pi-musubi-core` changes.
Foundation PR #348 provided the prior one-use downstream handoff authority for C1 first positive source scope implementation-repo work only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #160.
No remaining work may inherit permission from foundation PR #348 or implementation PR #160.
The PR #348 allowance authorized only `docs/foundation_lock.md`, `apps/backend/crates/social-trust-domain/src/lib.rs`, `apps/backend/crates/social-trust-domain/tests/c1_first_positive_source_scope_contract.rs`, and existing `apps/backend/crates/social-trust-domain/tests/*.rs` only if extending an existing Social Trust domain contract is smaller and clearer.
It authorized only pure Social Trust domain classification for the C1 first positive source scope plus deterministic Social Trust domain contract verification.
The only accepted positive source labels for this PR are `promise_reliability_outcome.completed_as_agreed` and `promise_reliability_outcome.completed_after_governed_review`.
No other source label may be treated as part of the C1 first positive source scope.
That handoff did not authorize new source facts, new mutation facts, numeric Social Trust scores, weights, ranks, display levels, public display, recommendation boost, discovery priority, contact unlock, room transition, settlement progression, Promise runtime behavior, proof runtime behavior, proof-derived Social Trust or Relationship Depth behavior, Relationship Depth facts or mutation behavior, projection rows, projection refresh, public API routes, mobile UI, schema-only work, DDL, migrations, provider guarantees, new durable product vocabulary in core docs, or broader `pi-musubi-core` changes.
The foundation closeout for that consumed allowance is recorded in `docs/readiness/c1_social_trust_positive_source_implementation_closeout_ledger.md`.

Foundation PR #430 provided the consumed one-use downstream docs-only lock alignment authority for one implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #164 and closed out by foundation PR #432.
Foundation PR #434 provided the consumed one-use downstream test-only hard-exclusion authority for one implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #166 and closed out by foundation PR #436.
Foundation PR #438 provided the consumed one-use downstream narrow writer fact persistence handoff authority for one implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #167 and closed out by foundation PR #439.
No remaining work may inherit permission from foundation PR #438 or implementation PR #167.
Foundation PR #441 selected post-writer-fact-persistence foundation continuation only.
Foundation PR #443 prepared state transition runtime preflight decision materials only.
Foundation PR #445 found that preflight packet sufficient for later route selection only.
Foundation PR #447 provides the current one-use downstream narrow state transition runtime handoff authority for this implementation-repo PR only.
This implementation-repo PR consumes that allowance.
The PR #447 allowance authorizes only:

- `docs/foundation_lock.md`
- `apps/backend/src/services/promise_completion/mod.rs`
- `apps/backend/src/services/promise_completion/repository.rs`
- `apps/backend/src/services/promise_completion/types.rs`
- `apps/backend/tests/promise_completion_state_transition_runtime.rs`

It authorizes only an internal Promise completion helper for the pre-bounded mutual acknowledgement accepted transition:

```text
completion_pending_mutual_acknowledgement -> completion_accepted
```

under:

```text
mutual_accountable_completion_acknowledgement
```

It must use the existing `promise_completion.writer_fact_records` table only.
It authorizes only one append-only `completion_state_transition` writer fact record for the selected transition attempt, with durable database-enforced idempotency, identical replay, payload-drift fail-closed behavior, writer-truth prior posture validation, PII / raw-evidence / provider-payload segregation, and database contract tests.
It must set `completed_reference_eligible = true` only because the next state is `completion_accepted`.
It does not authorize migrations, DDL, new tables, mutable current-state tables, `apps/backend/src/services/mod.rs`, `apps/backend/src/lib.rs`, handlers, routers, `AppState`, source-route evaluation runtime, participant acknowledgement collection runtime, governed review runtime workflow, governed review accepted transition runtime, correction or supersession runtime, API, UI, projection, Social Trust source fact persistence, Social Trust mutation, Relationship Depth mutation, settlement movement, room behavior, contact behavior, discovery, recommendation, proof runtime behavior, proof eligibility runtime behavior, outbox, inbox, worker behavior, provider I/O, provider callbacks, raw Personal Data in immutable truth, raw evidence in immutable truth, hidden distributed transactions, destructive migration, Social Trust score, weight, rank, public display, recovery ceiling, settlement release, settlement refund, settlement forfeiture, escrow movement, reward movement, room progression, direct-message unlock, public accusation labels, sensitive-trait visibility changes, paid romantic advantage, payment-based direct-message unlock, or broad `pi-musubi-core` changes.
After this downstream PR is merged, closed without merge, or replaced by a different accepted foundation decision, foundation must receive a separate closeout ledger at `docs/readiness/promise_completion_narrow_state_transition_runtime_closeout_ledger.md` before any broader Promise completion route may be considered.

The C2 and post-C2 readiness and closeout chain is accepted for docs-only foundation semantic scope:

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
- `docs/readiness/post_c2_categorical_fact_projection_api_non_exposure_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_pii_retention_non_leakage_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_pii_retention_non_leakage_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_pii_retention_non_leakage_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_idempotency_replay_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_idempotency_replay_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_idempotency_replay_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_lifecycle_replay_boundary_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_lifecycle_replay_boundary_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_lifecycle_replay_boundary_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_rejection_boundary_non_persistence_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_rejection_boundary_non_persistence_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_rejection_boundary_non_persistence_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_concurrent_idempotency_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_concurrent_idempotency_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_concurrent_idempotency_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_subject_realm_idempotency_scope_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_subject_realm_idempotency_scope_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_subject_realm_idempotency_scope_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_subject_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_subject_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_subject_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_reclassification_replay_evidence_package.md`
- `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_reclassification_replay_handoff_gate_decision.md`
- `docs/readiness/post_c2_categorical_fact_controlled_exceptional_account_reclassification_replay_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_controlled_exceptional_account_promise_participant_exclusion_evidence_package.md`
- `docs/readiness/post_c2_controlled_exceptional_account_promise_participant_exclusion_handoff_gate_decision.md`
- `docs/readiness/post_c2_controlled_exceptional_account_promise_participant_exclusion_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_legal_hold_writer_fact_boundary_evidence_package.md`
- `docs/readiness/post_c2_legal_hold_writer_fact_boundary_handoff_gate_decision.md`
- `docs/readiness/post_c2_legal_hold_writer_fact_shape_evidence_package.md`
- `docs/readiness/post_c2_legal_hold_writer_fact_shape_handoff_gate_decision.md`
- `docs/readiness/post_c2_legal_hold_writer_fact_downstream_scope_evidence_package.md`
- `docs/readiness/post_c2_legal_hold_writer_fact_downstream_scope_handoff_gate_decision.md`
- `docs/readiness/post_c2_master_submaster_operator_seat_writer_fact_lock_alignment_handoff_gate_decision.md`
- `docs/readiness/post_c2_master_submaster_operator_seat_runtime_implementation_handoff_gate_decision.md`
- `docs/readiness/post_c2_master_submaster_operator_seat_runtime_non_authority_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_master_submaster_operator_seat_active_authority_writer_fact_shape_evidence_package.md`
- `docs/readiness/post_c2_legal_privacy_consumer_contract_quantitative_gate_evidence_package.md`
- `docs/readiness/post_c2_master_submaster_operator_seat_active_authority_writer_fact_shape_handoff_gate_decision.md`
- `docs/readiness/post_c2_legal_privacy_consumer_contract_quantitative_gate_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_command_lease_reclaim_test_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_command_lease_reclaim_test_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_nonterminal_preservation_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_nonterminal_preservation_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_nonterminal_preservation_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_terminal_archive_payload_preservation_evidence_package.md`
- `docs/readiness/post_c2_orchestration_terminal_archive_payload_preservation_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_terminal_archive_payload_preservation_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_terminal_quarantine_archive_diagnostics_preservation_evidence_package.md`
- `docs/readiness/post_c2_orchestration_terminal_quarantine_archive_diagnostics_preservation_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_terminal_quarantine_archive_diagnostics_preservation_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_idempotency_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_idempotency_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_idempotency_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_terminal_prune_retention_eligibility_evidence_package.md`
- `docs/readiness/post_c2_orchestration_terminal_prune_retention_eligibility_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_terminal_prune_retention_eligibility_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_mixed_eligibility_separation_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_mixed_eligibility_separation_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_mixed_eligibility_separation_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_deterministic_outcome_ordering_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_deterministic_outcome_ordering_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_deterministic_outcome_ordering_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_outbox_attempt_archive_completeness_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_outbox_attempt_archive_completeness_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_outbox_attempt_archive_completeness_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_formatting_compliance_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_formatting_compliance_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_command_inbox_archive_completeness_formatting_compliance_implementation_closeout_ledger.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_fail_closed_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_fail_closed_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_corrective_evidence_package.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_corrective_handoff_gate_decision.md`
- `docs/readiness/post_c2_orchestration_prune_archive_conflict_mismatch_side_effect_containment_corrective_implementation_closeout_ledger.md`
- `docs/readiness/c1_trust_depth_proof_authority_foundation_evidence_package.md`
- `docs/readiness/c1_trust_depth_proof_authority_source_law_ledger.md`
- `docs/readiness/c1_trust_depth_proof_authority_non_authorization_guard.md`
- `docs/readiness/c1_trust_depth_proof_authority_test_only_handoff_gate_decision.md`
- `docs/operations/readiness_routine.md`
- `docs/operations/runalways_readiness_orchestrator_design.md`
- `docs/operations/runalways_readiness_orchestrator_stage0.md`
- `docs/operations/runalways_lane_controller_v0_1.md`

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
Foundation PR #130 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only, limited to post-C2 categorical Social Trust fact projection API non-exposure verification.
That allowance was consumed by `mt4110/pi-musubi-core` PR #94 and closed out by foundation PR #132.
Foundation PR #134 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact PII / retention / evidence-segregation non-leakage verification.
Foundation PR #136 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only, limited to post-C2 categorical Social Trust fact PII / retention / evidence-segregation non-leakage verification.
That allowance was consumed by `mt4110/pi-musubi-core` PR #96 and closed out by foundation PR #138.
Foundation PR #140 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact durable idempotency / replay / payload-drift verification.
Foundation PR #142 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only, limited to post-C2 categorical Social Trust fact durable idempotency / replay / payload-drift verification.
That allowance was consumed by `mt4110/pi-musubi-core` PR #98 and closed out by foundation PR #144.
Foundation PR #146 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact account-lifecycle replay boundary verification.
Foundation PR #148 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only, limited to post-C2 categorical Social Trust fact account-lifecycle replay boundary verification.
That allowance was consumed by `mt4110/pi-musubi-core` PR #100 and closed out by foundation PR #150.
Foundation PR #152 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact rejection boundary non-persistence verification.
Foundation PR #154 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only, limited to post-C2 categorical Social Trust fact rejection boundary non-persistence verification.
That allowance was consumed by `mt4110/pi-musubi-core` PR #102 and closed out by foundation PR #156.
Foundation PR #158 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact concurrent idempotency / replay / payload-drift verification.
Foundation PR #160 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only, limited to post-C2 categorical Social Trust fact concurrent idempotency / replay / payload-drift verification.
That allowance was consumed by `mt4110/pi-musubi-core` PR #104 and closed out by foundation PR #162.
Foundation PR #164 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact subject and Realm idempotency scope verification.
Foundation PR #166 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #106 and closed out by foundation PR #168.
Foundation PR #170 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact Controlled Exceptional Account subject non-participation verification.
Foundation PR #173 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #108 and closed out by foundation PR #175.
Foundation PR #177 accepted the exact candidate test-only slice as post-C2 categorical Social Trust fact Controlled Exceptional Account reclassification replay boundary verification.
Foundation PR #179 added an advisory readiness routine runner and did not grant GO, implementation handoff, or implementation authority.
Foundation PR #181 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #110 and closed out by foundation PR #183.
Foundation PR #185 accepted the exact candidate test-only slice as post-C2 Controlled Exceptional Account Promise participant exclusion verification.
Foundation PR #189 added the Stage 0 report-only RunAlways runner and did not grant GO, implementation handoff, implementation authority, or automation authority.
Foundation PR #191 provided the consumed narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #111 and closed out by foundation PR #193.
Foundation PR #195 accepted the foundation-side Legal Hold writer fact boundary evidence package only and did not authorize `pi-musubi-core` changes.
Foundation PR #197 kept the Legal Hold writer fact boundary handoff result at NO-GO and did not authorize foundation lock alignment or downstream work.
Foundation PR #199 accepted the foundation-side Legal Hold writer fact shape evidence package only and did not authorize `pi-musubi-core` changes.
Foundation PR #201 kept the Legal Hold writer fact shape handoff result at NO-GO and did not authorize foundation lock alignment or downstream work.
Foundation PR #203 accepted the foundation-side Legal Hold writer fact downstream scope evidence package only and did not authorize `pi-musubi-core` changes.
Foundation PR #205 provided the prior narrow downstream docs-only handoff authority for one later implementation-repo PR only.
This update is the required lock pin for that one-use allowance.
It authorizes only `docs/foundation_lock.md` and `apps/backend/docs/post_c2_legal_hold_writer_fact_non_authority_scope.md`.
It does not authorize runtime tests, schema-only work, DDL, migrations, backend runtime code, backend README updates, public API changes, mobile UI, projection refresh, runtime orchestration, retry workers, queues, outbox changes, inbox changes, lifecycle runtime behavior, pruning, archive, deletion, Legal Hold runtime behavior, key lifecycle behavior, evidence access runtime behavior, active Legal Hold writer fact creation, invalid Legal Hold rejection persistence, Social Trust source facts, Social Trust mutation facts, Relationship Depth facts, discovery, recommendation, room, settlement, Promise runtime behavior, proof runtime behavior, Social Trust scoring, public trust display, or any broader `pi-musubi-core` change.
That allowance was consumed by `mt4110/pi-musubi-core` PR #112 and closed out by foundation PR #207.
Foundation PR #208 accepted the foundation-side Master / Submaster operator seat authority boundary evidence package only and did not authorize `pi-musubi-core` changes.
Foundation PR #209 kept the Master / Submaster operator seat authority boundary handoff result at NO-GO and did not authorize foundation lock alignment or downstream work.
Foundation PR #210 canonized Master and Submaster as internal backstage operator-seat terms and did not authorize runtime implementation or downstream work.
Foundation PR #211 accepted ADR-0037 for the Master / Submaster operator-seat writer fact boundary and did not authorize runtime implementation or downstream work.
Foundation PR #212 provided the prior narrow downstream docs-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #113.
That allowance authorized only `docs/foundation_lock.md`.
It did not authorize runtime tests, schema-only work, DDL, migrations, backend runtime code, backend README updates, public API changes, mobile UI, projection refresh, runtime orchestration, retry workers, queues, outbox changes, inbox changes, lifecycle runtime behavior, pruning, archive, deletion, Legal Hold runtime behavior, evidence access runtime behavior, key lifecycle behavior, external counsel runtime behavior, court-facing workflow, legal-process intake, treasury behavior, compensation behavior, active Master / Submaster writer fact creation, active Legal Hold writer fact creation, evidence access grant or audit runtime creation, Social Trust source facts, Social Trust mutation facts, Relationship Depth facts, discovery, recommendation, room, settlement, Promise runtime behavior, proof runtime behavior, Social Trust scoring, public trust display, or any broader `pi-musubi-core` change.
Foundation PR #213 provided the prior narrow downstream docs-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #114 and closed out by foundation PR #214.
Foundation PR #214 returned runtime implementation, runtime tests, test-only work, schema-only work, DDL, migrations, backend runtime code, public API changes, mobile UI, projection refresh, runtime orchestration, retry workers, queues, outbox changes, inbox changes, lifecycle runtime behavior, pruning, archive, deletion, Legal Hold runtime behavior, evidence access runtime behavior, key lifecycle behavior, Social Trust, Relationship Depth, discovery, recommendation, room, settlement, Promise runtime, proof runtime, Social Trust scoring, public trust display, and broader `pi-musubi-core` changes to NO-GO.
Foundation PR #216 accepted the foundation-side Master / Submaster active authority writer fact shape evidence package and the legal / privacy / consumer-contract quantitative gate evidence package only.
Foundation PR #217 kept the Master / Submaster active authority writer fact shape handoff result at NO-GO and did not authorize foundation lock alignment, downstream docs-only work, test-only work, schema-only work, DDL, migrations, runtime tests, backend code, public API changes, mobile UI, projection refresh, runtime orchestration, or `pi-musubi-core` changes.
Foundation PR #218 accepted the legal / privacy / consumer-contract quantitative gate as a required future gate shape only.
It did not authorize runtime implementation, DDL, migrations, runtime tests, gate invocation for implementation, runtime handoff, implementation handoff, backend code, public API changes, mobile UI, projection refresh, runtime orchestration, Terms of Service finalization, Privacy Policy finalization, blockchain anchoring, foundation lock alignment, downstream docs-only work, or `pi-musubi-core` changes.
Foundation PR #221 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #142 and closed out by foundation PR #223.
Foundation PR #227 accepted the exact candidate test-only slice as post-C2 orchestration coordination prune nonterminal preservation verification.
Foundation PR #229 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That consumed lock update authorized only `docs/foundation_lock.md`, `apps/backend/crates/orchestration/tests/postgres_contract.rs`, `apps/backend/docs/guardrails.md`, and `apps/backend/docs/raw_transaction_inventory.txt`.
It allowed only deterministic PostgreSQL-backed contract verification that existing coordination pruning preserves pending and processing outbox / command inbox coordination rows and does not archive or delete them as terminal coordination rows, plus the two named backend-local guardrail documentation updates.
It did not authorize broad runtime implementation, DDL, migrations, backend runtime code, public API changes, mobile UI, projection refresh, new runtime orchestration behavior, retry workers, queues, outbox changes, inbox changes, lifecycle runtime behavior, pruning runtime behavior, archive runtime behavior, deletion, Legal Hold runtime behavior, evidence access runtime behavior, key lifecycle behavior, discovery, recommendation, room, settlement, Promise runtime behavior, proof runtime behavior, Relationship Depth behavior, Social Trust scoring, public trust display, or any broader `pi-musubi-core` change.
That allowance was consumed by `mt4110/pi-musubi-core` PR #143 and closed out by foundation PR #231.
Foundation PR #233 accepted the exact candidate test-only slice as post-C2 orchestration terminal archive payload preservation verification.
Foundation PR #235 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That consumed lock update authorized only `docs/foundation_lock.md`, `apps/backend/crates/orchestration/tests/postgres_contract.rs`, `apps/backend/docs/guardrails.md`, and `apps/backend/docs/raw_transaction_inventory.txt`.
It allowed only deterministic PostgreSQL-backed contract verification that existing coordination pruning preserves terminal coordination archive payload and correlation evidence when existing hot-table coordination rows are moved to archive tables before pruning, plus the two named backend-local guardrail documentation updates.
It did not authorize broad runtime implementation, DDL, migrations, backend runtime code, public API changes, mobile UI, projection refresh, new runtime orchestration behavior, retry workers, queues, outbox changes, inbox changes, archive schema changes, manual-review provider evidence pruning behavior, lifecycle runtime behavior, pruning runtime behavior, archive runtime behavior, deletion, Legal Hold runtime behavior, evidence access runtime behavior, key lifecycle behavior, discovery, recommendation, room, settlement, Promise runtime behavior, proof runtime behavior, Relationship Depth behavior, Social Trust scoring, public trust display, or any broader `pi-musubi-core` change.
That allowance was consumed by `mt4110/pi-musubi-core` PR #144 and closed out by foundation PR #237.
Foundation PR #239 accepted the exact candidate test-only slice as post-C2 orchestration terminal quarantine archive diagnostics preservation verification.
Foundation PR #241 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That consumed lock update authorized only `docs/foundation_lock.md`, `apps/backend/crates/orchestration/tests/postgres_contract.rs`, `apps/backend/docs/guardrails.md`, and `apps/backend/docs/raw_transaction_inventory.txt`.
It allowed only deterministic PostgreSQL-backed contract verification that existing coordination pruning preserves terminal quarantine and failure diagnostics when existing hot-table coordination rows are moved to archive tables before pruning, plus the two named backend-local guardrail documentation updates.
It does not authorize broad runtime implementation, DDL, migrations, backend runtime code, public API changes, mobile UI, projection refresh, new runtime orchestration behavior, retry workers, queues, outbox changes, inbox changes, archive schema changes, manual-review provider evidence pruning behavior, lifecycle runtime behavior, pruning runtime behavior, archive runtime behavior, deletion, Legal Hold runtime behavior, evidence access runtime behavior, key lifecycle behavior, discovery, recommendation, room, settlement, Promise runtime behavior, proof runtime behavior, Relationship Depth behavior, Social Trust scoring, public trust display, or any broader `pi-musubi-core` change.
That allowance was consumed by `mt4110/pi-musubi-core` PR #145 and closed out by foundation PR #243.
Foundation PR #245 accepted the exact candidate test-only slice as post-C2 orchestration prune archive conflict idempotency verification.
Foundation PR #247 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #146 and closed out by foundation PR #249.
Foundation PR #251 accepted the exact candidate test-only slice as post-C2 orchestration terminal prune retention eligibility verification.
Foundation PR #253 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #147 and closed out by foundation PR #255.
Foundation PR #257 accepted the exact candidate test-only slice as post-C2 orchestration prune mixed eligibility separation verification.
Foundation PR #259 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #148 and closed out by foundation PR #261.
Foundation PR #263 accepted the exact candidate test-only slice as post-C2 orchestration prune deterministic outcome ordering verification.
Foundation PR #265 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #149 and closed out by foundation PR #267.
Foundation PR #269 accepted the exact candidate test-only slice as post-C2 orchestration prune outbox attempt archive completeness verification.
Foundation PR #271 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #150 and closed out by foundation PR #273.
Foundation PR #275 accepted the exact candidate test-only slice as post-C2 orchestration prune command inbox archive completeness verification.
Foundation PR #277 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #151 and closed out by foundation PR #279 with a formatting validation gap recorded.
Foundation PR #281 accepted the exact candidate corrective slice as post-C2 orchestration prune command inbox archive completeness formatting compliance correction.
Foundation PR #283 provided the prior narrow downstream formatting-only handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #152 and closed out by foundation PR #285.
Foundation PR #287 accepted the exact candidate test-only slice as post-C2 orchestration prune archive conflict mismatch fail-closed verification.
Foundation PR #289 provided the prior narrow downstream test-only handoff authority for one later implementation-repo PR only, or a local proof attempt that would stop and return to foundation if the deterministic test failed before PR creation.
That one-use test-only allowance was consumed locally, failed before PR creation, and returned to foundation without opening an implementation-repo PR or patching runtime code.
Foundation PR #291 accepted the corrective evidence package recording that failing proof.
Foundation PR #293 provided the prior narrow downstream corrective runtime handoff authority for one later implementation-repo PR only.
That allowance was consumed by `mt4110/pi-musubi-core` PR #153 and closed out by foundation PR #295.
Foundation PR #297 accepted the exact candidate test-only slice as post-C2 orchestration prune archive conflict mismatch side-effect containment verification.
Foundation PR #299 provided the prior one-use test-only local proof allowance.
That local proof failed before implementation-repo PR creation and returned to foundation without patching runtime code.
Foundation PR #301 accepted the corrective evidence package recording that failing proof.
Foundation PR #303 provided the prior narrow downstream corrective runtime handoff authority for one later implementation-repo PR only.
That allowance authorized only `docs/foundation_lock.md`, `apps/backend/crates/orchestration/src/postgres.rs`, `apps/backend/crates/orchestration/tests/postgres_contract.rs`, `apps/backend/docs/guardrails.md`, and `apps/backend/docs/raw_transaction_inventory.txt`.
It allowed only updating this foundation lock to pin PR #303, correcting the existing PostgreSQL prune path so preexisting archive rows under the same keys must be evidence-equivalent before any additional archive rows are inserted or corresponding terminal hot coordination rows are deleted or reported as pruned, adding the deterministic PostgreSQL contract test named `postgres_prune_archive_conflict_mismatch_does_not_expand_archives`, preserving the accepted archive conflict mismatch fail-closed behavior, preserving the accepted matching archive conflict idempotency behavior, and updating the named backend-local guardrail documents.
That allowance was consumed by `mt4110/pi-musubi-core` PR #155 and closed out by foundation PR #305.
Foundation PR #305 records that PR #303 is fully consumed and that no later work may reuse PR #303 as runtime implementation authority.
Foundation PR #305 returns broad runtime implementation, runtime tests, DDL, migrations, implementation handoff, runtime handoff, gate invocation, backend code, public API changes, mobile UI, projection refresh, runtime orchestration, retry workers, queues, outbox changes, inbox changes, archive schema changes, retention policy changes, lifecycle runtime behavior, pruning runtime behavior, archive runtime behavior, deletion, Legal Hold runtime behavior, evidence access runtime behavior, key lifecycle behavior, provider guarantees, discovery, recommendation, room, settlement, Promise runtime, proof runtime, Relationship Depth, Social Trust scoring/display, paid romantic advantage, DM unlock by payment, and broader `pi-musubi-core` changes to NO-GO.
Foundation PR #313 authorized one later downstream docs-only PR limited to this file, `docs/foundation_lock.md`, to pin foundation PR #305 closeout.
This update consumes that one-use foundation lock alignment allowance.

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

The Trust / Depth, Proof / Evidence, Server / Realm / Citadel / Pool, Discovery, Recommendation, and Master / Submaster operator-seat foundation tranches are FULL FOUNDATION PASS records for accepted foundation scope through ADR-0037.

This does not implement Social Trust runtime behavior, Relationship Depth runtime behavior, proof writer runtime behavior, Server alias runtime behavior, Citadel binding runtime behavior, Authority Lease runtime behavior, Realm relocation runtime behavior, Pool attribution runtime behavior, discovery runtime behavior, recommendation runtime behavior, trust/depth contamination guards, Master / Submaster runtime behavior, active Master / Submaster writer fact creation, Legal Hold runtime behavior, or evidence access runtime behavior.

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
The post-C2 categorical fact projection API non-exposure evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #94 and closed out by foundation PR #132.
The post-C2 categorical fact PII retention non-leakage evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #96 and closed out by foundation PR #138.
The post-C2 categorical fact idempotency replay evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #98 and closed out by foundation PR #144.
The post-C2 categorical fact lifecycle replay boundary evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #100 and closed out by foundation PR #150.
The post-C2 categorical fact rejection boundary non-persistence evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #102 and closed out by foundation PR #156.
The post-C2 categorical fact concurrent idempotency evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #104 and closed out by foundation PR #162.
The post-C2 categorical fact subject and Realm idempotency scope evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #106 and closed out by foundation PR #168.
The post-C2 categorical fact Controlled Exceptional Account subject evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #108 and closed out by foundation PR #175.
The post-C2 categorical fact Controlled Exceptional Account reclassification replay evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #110 and closed out by foundation PR #183.
The post-C2 Controlled Exceptional Account Promise participant exclusion evidence package and handoff gate authorized one later implementation-repo test-only PR only.
That test-only allowance was consumed by `mt4110/pi-musubi-core` PR #111 and closed out by foundation PR #193.
The post-C2 Legal Hold writer fact boundary and shape evidence / handoff sequence preserved downstream implementation NO-GO through foundation PR #203.
The post-C2 Legal Hold writer fact downstream scope handoff gate now authorizes one later implementation-repo docs-only PR only.
This docs-only allowance is limited to `docs/foundation_lock.md` and `apps/backend/docs/post_c2_legal_hold_writer_fact_non_authority_scope.md`.
It may record that existing hold-like labels, review labels, settlement hold language, Social Trust boundary references, projection state, observability, provider callbacks, proof evidence, support tickets, issue comments, operator notes, client state, and frontend state are not ADR-0011 Legal Hold writer facts.
It may record that `mt4110/pi-musubi-core#64` remains open and not implementation-ready.
It does not authorize tests, schema-only work, DDL, migrations, backend runtime code, backend README updates, public API changes, mobile UI, projection refresh, runtime orchestration, lifecycle runtime behavior, pruning, archive, deletion, Legal Hold runtime behavior, key lifecycle behavior, evidence access runtime behavior, active Legal Hold writer fact creation, invalid Legal Hold rejection persistence, Social Trust source facts, Social Trust mutation facts, Relationship Depth facts, discovery, recommendation, room progression, settlement behavior, Promise runtime behavior, proof runtime behavior, Social Trust scoring, public trust display, or broad runtime implementation.

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
- Updated from foundation SHA: `800a69c810e48646c583d2cdd657001d011ff759` -> `a5a55ee8b86dc20b04890f302bc062301e2e1f6c`
- Reason: Align implementation-repo lock with the accepted Promise completion narrow state transition runtime route selection after foundation PR #447 (`docs: select Promise completion state transition route`) and consume its one-use downstream mutual acknowledgement accepted transition allowance.
- New required docs: Promise completion narrow writer fact persistence closeout ledger, Promise completion post-writer-fact-persistence closeout route selection, Promise completion state transition runtime preflight decision packet, Promise completion state transition runtime preflight packet sufficiency decision, and Promise completion post-state-transition-preflight sufficiency route selection.
- Removed docs: None.
- Implementation impact: This update consumes the one-use foundation PR #447 allowance and permits only the exact five-file narrow state transition scope listed above. It authorizes only an internal mutual acknowledgement accepted transition helper that writes one append-only `completion_state_transition` fact to the existing `promise_completion.writer_fact_records` table, with durable database-enforced idempotency, identical replay, payload drift fail-closed behavior, writer-truth prior posture validation, PII / raw-evidence / provider-payload segregation, and database contract tests. Migrations, DDL, new tables, source-route evaluation runtime, participant acknowledgement collection runtime, governed review runtime, correction/supersession runtime, API, UI, projection, Social Trust source fact persistence, Social Trust mutation, Relationship Depth mutation, settlement movement, room behavior, contact behavior, discovery, recommendation, outbox, inbox, worker behavior, provider I/O, provider callbacks, participant display implementation, paid romantic advantage, payment-based direct-message unlock, and broader `pi-musubi-core` changes remain NO-GO.
- Review completed by: Masaki Takemura

---

## 7. One-line memory

`musubi-foundation` tells us what MUSUBI is.
`pi-musubi-core` is only allowed to make that meaning executable.
