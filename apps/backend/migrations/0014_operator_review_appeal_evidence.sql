-- Design source: ISSUE-12-operator-review-appeal-evidence.md
-- GitHub issue number is intentionally not hardcoded.

CREATE TABLE IF NOT EXISTS core.operator_role_assignments (
    operator_role_assignment_id UUID PRIMARY KEY,
    operator_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    operator_role TEXT NOT NULL CHECK (
        operator_role IN ('reviewer', 'approver', 'steward', 'auditor', 'support')
    ),
    grant_reason TEXT NOT NULL CHECK (char_length(trim(grant_reason)) > 0),
    granted_by_operator_id UUID REFERENCES core.accounts(account_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    revoked_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS operator_role_assignments_active_unique
    ON core.operator_role_assignments (operator_account_id, operator_role)
    WHERE revoked_at IS NULL;

COMMENT ON TABLE core.operator_role_assignments IS
'Controlled operator role grants for ISSUE-12 review/evidence workflows. Roles are separated even when one Day 1 human holds multiple responsibilities.';

CREATE TABLE IF NOT EXISTS dao.review_cases (
    review_case_id UUID PRIMARY KEY,
    case_type TEXT NOT NULL CHECK (
        case_type IN (
            'proof_anomaly',
            'promise_dispute',
            'settlement_conflict',
            'safety_escalation',
            'realm_admission_review',
            'operator_manual_hold',
            'sealed_room_fallback',
            'appeal'
        )
    ),
    severity TEXT NOT NULL CHECK (severity IN ('sev0', 'sev1', 'sev2', 'sev3')),
    review_status TEXT NOT NULL CHECK (
        review_status IN (
            'open',
            'triaged',
            'under_review',
            'awaiting_evidence',
            'decided',
            'appealed',
            'closed'
        )
    ),
    subject_account_id UUID REFERENCES core.accounts(account_id),
    related_promise_intent_id UUID REFERENCES dao.promise_intents(promise_intent_id),
    related_settlement_case_id UUID REFERENCES dao.settlement_cases(settlement_case_id),
    related_realm_id TEXT,
    opened_reason_code TEXT NOT NULL CHECK (
        opened_reason_code IN (
            'verification_pending_review',
            'proof_rejected_expired',
            'promise_completion_under_review',
            'manual_hold_safety_review',
            'appeal_received',
            'proof_missing',
            'proof_inconclusive',
            'safety_review',
            'policy_review',
            'duplicate_or_invalid',
            'resolved_no_action',
            'restricted_after_review',
            'restored_after_review'
        )
    ),
    source_fact_kind TEXT NOT NULL CHECK (char_length(trim(source_fact_kind)) > 0),
    source_fact_id TEXT NOT NULL CHECK (char_length(trim(source_fact_id)) > 0),
    source_snapshot_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    assigned_operator_id UUID REFERENCES core.accounts(account_id),
    opened_by_operator_id UUID REFERENCES core.accounts(account_id),
    request_idempotency_key TEXT,
    opened_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS review_cases_idempotency_unique
    ON dao.review_cases (opened_by_operator_id, request_idempotency_key)
    WHERE request_idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS review_cases_queue_idx
    ON dao.review_cases (review_status, severity, opened_at);

COMMENT ON TABLE dao.review_cases IS
'ISSUE-12 review cases. They reference source facts but do not overwrite Promise, settlement, proof, or other authoritative writer truth.';

CREATE TABLE IF NOT EXISTS dao.evidence_bundles (
    evidence_bundle_id UUID PRIMARY KEY,
    review_case_id UUID NOT NULL REFERENCES dao.review_cases(review_case_id),
    evidence_visibility TEXT NOT NULL CHECK (
        evidence_visibility IN ('summary_only', 'redacted_raw', 'full_raw')
    ),
    summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    raw_locator_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    retention_class TEXT NOT NULL CHECK (retention_class IN ('R4', 'R6', 'R7')),
    created_by_operator_id UUID REFERENCES core.accounts(account_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS evidence_bundles_review_case_idx
    ON dao.evidence_bundles (review_case_id, created_at);

COMMENT ON TABLE dao.evidence_bundles IS
'Logical evidence containers for ISSUE-12. User-facing read models must not expose raw_locator_json.';

CREATE TABLE IF NOT EXISTS dao.evidence_access_grants (
    access_grant_id UUID PRIMARY KEY,
    review_case_id UUID NOT NULL REFERENCES dao.review_cases(review_case_id),
    evidence_bundle_id UUID REFERENCES dao.evidence_bundles(evidence_bundle_id),
    grantee_operator_id UUID NOT NULL REFERENCES core.accounts(account_id),
    access_scope TEXT NOT NULL CHECK (
        access_scope IN ('summary_only', 'redacted_raw', 'full_raw')
    ),
    grant_reason TEXT NOT NULL CHECK (char_length(trim(grant_reason)) > 0),
    approved_by_operator_id UUID NOT NULL REFERENCES core.accounts(account_id),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at > created_at)
);

CREATE INDEX IF NOT EXISTS evidence_access_grants_grantee_idx
    ON dao.evidence_access_grants (grantee_operator_id, review_case_id, expires_at)
    WHERE revoked_at IS NULL;

COMMENT ON TABLE dao.evidence_access_grants IS
'Scoped and auditable evidence access grants. Grants are separate from case existence and expire by design.';

CREATE TABLE IF NOT EXISTS dao.operator_decision_facts (
    operator_decision_fact_id UUID PRIMARY KEY,
    review_case_id UUID NOT NULL REFERENCES dao.review_cases(review_case_id),
    appeal_case_id UUID,
    decision_kind TEXT NOT NULL CHECK (
        decision_kind IN (
            'no_action',
            'uphold',
            'restrict',
            'restore',
            'request_more_evidence',
            'escalate'
        )
    ),
    user_facing_reason_code TEXT NOT NULL CHECK (
        user_facing_reason_code IN (
            'verification_pending_review',
            'proof_rejected_expired',
            'promise_completion_under_review',
            'manual_hold_safety_review',
            'appeal_received',
            'proof_missing',
            'proof_inconclusive',
            'safety_review',
            'policy_review',
            'duplicate_or_invalid',
            'resolved_no_action',
            'restricted_after_review',
            'restored_after_review'
        )
    ),
    operator_note_internal TEXT,
    decision_payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    decided_by_operator_id UUID NOT NULL REFERENCES core.accounts(account_id),
    decision_idempotency_key TEXT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS operator_decision_facts_idempotency_unique
    ON dao.operator_decision_facts (review_case_id, decided_by_operator_id, decision_idempotency_key)
    WHERE decision_idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS operator_decision_facts_case_idx
    ON dao.operator_decision_facts (review_case_id, recorded_at);

COMMENT ON TABLE dao.operator_decision_facts IS
'Append-only ISSUE-12 operator decision facts. Operator actions must add facts rather than rewrite original writer truth.';

CREATE TABLE IF NOT EXISTS dao.appeal_cases (
    appeal_case_id UUID PRIMARY KEY,
    source_review_case_id UUID NOT NULL REFERENCES dao.review_cases(review_case_id),
    source_decision_fact_id UUID REFERENCES dao.operator_decision_facts(operator_decision_fact_id),
    appeal_status TEXT NOT NULL CHECK (
        appeal_status IN ('submitted', 'accepted', 'under_review', 'decided', 'closed')
    ),
    submitted_by_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    submitted_reason_code TEXT NOT NULL CHECK (
        submitted_reason_code IN (
            'verification_pending_review',
            'proof_rejected_expired',
            'promise_completion_under_review',
            'manual_hold_safety_review',
            'appeal_received',
            'proof_missing',
            'proof_inconclusive',
            'safety_review',
            'policy_review',
            'duplicate_or_invalid',
            'resolved_no_action',
            'restricted_after_review',
            'restored_after_review'
        )
    ),
    appellant_statement TEXT,
    new_evidence_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    appeal_idempotency_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE dao.operator_decision_facts
    ADD CONSTRAINT operator_decision_facts_appeal_fk
    FOREIGN KEY (appeal_case_id) REFERENCES dao.appeal_cases(appeal_case_id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE UNIQUE INDEX IF NOT EXISTS appeal_cases_idempotency_unique
    ON dao.appeal_cases (source_review_case_id, submitted_by_account_id, appeal_idempotency_key)
    WHERE appeal_idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS appeal_cases_source_case_idx
    ON dao.appeal_cases (source_review_case_id, created_at);

COMMENT ON TABLE dao.appeal_cases IS
'ISSUE-12 appeal cases linked to the original review case or decision fact. Appeals do not destroy preserved evidence.';

CREATE TABLE IF NOT EXISTS projection.review_status_views (
    review_case_id UUID PRIMARY KEY,
    subject_account_id UUID,
    related_promise_intent_id UUID,
    related_settlement_case_id UUID,
    related_realm_id TEXT,
    user_facing_status TEXT NOT NULL CHECK (
        user_facing_status IN (
            'pending_review',
            'under_review',
            'decided',
            'appeal_available',
            'appeal_submitted',
            'evidence_requested',
            'sealed_or_restricted',
            'closed'
        )
    ),
    user_facing_reason_code TEXT NOT NULL CHECK (
        user_facing_reason_code IN (
            'verification_pending_review',
            'proof_rejected_expired',
            'promise_completion_under_review',
            'manual_hold_safety_review',
            'appeal_received',
            'proof_missing',
            'proof_inconclusive',
            'safety_review',
            'policy_review',
            'duplicate_or_invalid',
            'resolved_no_action',
            'restricted_after_review',
            'restored_after_review'
        )
    ),
    appeal_status TEXT NOT NULL CHECK (
        appeal_status IN ('none', 'appeal_available', 'submitted', 'under_review', 'decided', 'closed')
    ),
    latest_decision_fact_id UUID,
    evidence_requested BOOLEAN NOT NULL DEFAULT FALSE,
    appeal_available BOOLEAN NOT NULL DEFAULT FALSE,
    source_watermark_at TIMESTAMPTZ NOT NULL,
    source_fact_count BIGINT NOT NULL CHECK (source_fact_count >= 0),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS review_status_views_subject_idx
    ON projection.review_status_views (subject_account_id, last_projected_at);

COMMENT ON TABLE projection.review_status_views IS
'User-facing ISSUE-12 review status projection. It exposes bounded status and reason codes only; internal notes and raw evidence stay out of this read model.';
