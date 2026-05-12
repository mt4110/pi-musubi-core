CREATE SCHEMA IF NOT EXISTS social_trust;

COMMENT ON SCHEMA social_trust IS
'Writer-owned Social Trust intake facts. This schema does not contain Social Trust mutation facts, scores, ranks, display levels, Relationship Depth facts, or projection truth.';

CREATE TABLE IF NOT EXISTS social_trust.proposed_mutation_attempts (
    attempt_id UUID PRIMARY KEY,
    subject_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    source_category TEXT NOT NULL CHECK (
        source_category IN (
            'writer_source_candidate',
            'unknown',
            'projection_state',
            'analytics_state',
            'model_output',
            'observability_state',
            'client_state',
            'frontend_state',
            'payment_amount',
            'payment_frequency',
            'support_amount_or_status',
            'token_holdings',
            'popularity',
            'follower_count',
            'reply_speed',
            'dwell_time',
            'tenure',
            'romantic_desirability',
            'engagement',
            'engagement_telemetry',
            'recommendation_state',
            'discovery_state',
            'discovery_ranking',
            'relationship_depth',
            'room_projection',
            'operator_notes',
            'support_tickets',
            'issue_comments',
            'anti_abuse_marker_existence',
            'age_assurance_posture',
            'proof_callback_alone',
            'vendor_callback_alone',
            'controlled_exceptional_account_activity',
            'implementation_convenience'
        )
    ),
    writer_source_reference TEXT NOT NULL CHECK (char_length(trim(writer_source_reference)) > 0),
    reason_fact_reference TEXT NOT NULL CHECK (char_length(trim(reason_fact_reference)) > 0),
    attempt_idempotency_key TEXT NOT NULL CHECK (
        char_length(trim(attempt_idempotency_key)) > 0
    ),
    policy_version INTEGER NOT NULL CHECK (policy_version > 0),
    request_payload_hash TEXT NOT NULL CHECK (request_payload_hash ~ '^[0-9a-f]{64}$'),
    evidence_posture TEXT NOT NULL CHECK (evidence_posture IN ('bounded')),
    reviewability_posture TEXT NOT NULL CHECK (reviewability_posture IN ('reviewable')),
    retention_record_family TEXT NOT NULL CHECK (
        retention_record_family = 'Social Trust evidence or future Social Trust writer facts'
    ),
    retention_class_reference TEXT NOT NULL CHECK (
        char_length(trim(retention_class_reference)) > 0
    ),
    authority_posture TEXT NOT NULL CHECK (
        authority_posture IN ('writer_truth_only', 'projection_only')
    ),
    recorded_by_system TEXT NOT NULL DEFAULT 'social_trust_intake_persistence' CHECK (
        char_length(trim(recorded_by_system)) > 0
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE social_trust.proposed_mutation_attempts IS
'C1 Social Trust proposed mutation attempt intake records. These are intake/replay facts only, not Social Trust mutation truth.';

COMMENT ON COLUMN social_trust.proposed_mutation_attempts.subject_account_id IS
'Reference to an accepted account-continuity writer fact. This is not a Social Trust subject fact.';

COMMENT ON COLUMN social_trust.proposed_mutation_attempts.request_payload_hash IS
'SHA-256 hash of minimized intake meaning used for deterministic duplicate-delivery drift checks. Raw evidence and raw PII do not belong here.';

CREATE UNIQUE INDEX IF NOT EXISTS social_trust_attempt_dedupe_unique
    ON social_trust.proposed_mutation_attempts (
        subject_account_id,
        source_category,
        writer_source_reference,
        reason_fact_reference,
        policy_version,
        attempt_idempotency_key
    );

COMMENT ON INDEX social_trust.social_trust_attempt_dedupe_unique IS
'Durable database-enforced idempotency boundary for C1 Social Trust proposed mutation attempts.';

CREATE INDEX IF NOT EXISTS social_trust_attempt_subject_created_idx
    ON social_trust.proposed_mutation_attempts (subject_account_id, created_at);

CREATE INDEX IF NOT EXISTS social_trust_attempt_retention_idx
    ON social_trust.proposed_mutation_attempts (retention_record_family, retention_class_reference);

CREATE TABLE IF NOT EXISTS social_trust.intake_decisions (
    intake_decision_id UUID PRIMARY KEY,
    attempt_id UUID NOT NULL UNIQUE REFERENCES social_trust.proposed_mutation_attempts(attempt_id),
    decision_kind TEXT NOT NULL CHECK (
        decision_kind IN ('rejected', 'candidate_for_writer_persistence')
    ),
    rejection_reason_code TEXT CHECK (
        rejection_reason_code IS NULL
        OR rejection_reason_code IN (
            'forbidden_source',
            'unknown_source_category',
            'projection_only_authority',
            'missing_writer_source_reference',
            'missing_reason_fact',
            'missing_idempotency_posture',
            'missing_evidence_posture',
            'missing_reviewability_posture',
            'missing_retention_posture'
        )
    ),
    decision_payload_hash TEXT NOT NULL CHECK (decision_payload_hash ~ '^[0-9a-f]{64}$'),
    retention_record_family TEXT NOT NULL CHECK (
        retention_record_family = 'Social Trust evidence or future Social Trust writer facts'
    ),
    retention_class_reference TEXT NOT NULL CHECK (
        char_length(trim(retention_class_reference)) > 0
    ),
    recorded_by_system TEXT NOT NULL DEFAULT 'social_trust_intake_persistence' CHECK (
        char_length(trim(recorded_by_system)) > 0
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        (
            decision_kind = 'rejected'
            AND rejection_reason_code IS NOT NULL
        )
        OR (
            decision_kind = 'candidate_for_writer_persistence'
            AND rejection_reason_code IS NULL
        )
    )
);

COMMENT ON TABLE social_trust.intake_decisions IS
'C1 Social Trust intake decision records. CandidateForWriterPersistence is internal intake classification only and must not be treated as Social Trust mutation truth.';

COMMENT ON COLUMN social_trust.intake_decisions.decision_payload_hash IS
'SHA-256 hash of the minimized intake decision for deterministic replay checks.';

CREATE INDEX IF NOT EXISTS social_trust_intake_decision_kind_idx
    ON social_trust.intake_decisions (decision_kind, created_at);

CREATE INDEX IF NOT EXISTS social_trust_intake_decision_retention_idx
    ON social_trust.intake_decisions (retention_record_family, retention_class_reference);
