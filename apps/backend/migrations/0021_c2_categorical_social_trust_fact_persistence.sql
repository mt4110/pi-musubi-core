COMMENT ON SCHEMA social_trust IS
'Writer-owned Social Trust intake and C2 categorical mutation fact persistence. This schema does not contain numeric Social Trust scores, weights, ranks, display levels, Relationship Depth facts, or projection truth.';

CREATE TABLE IF NOT EXISTS social_trust.categorical_source_references (
    source_reference_id UUID PRIMARY KEY,
    subject_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    source_fact_label TEXT NOT NULL CHECK (
        source_fact_label IN (
            'promise_reliability_outcome.completed_as_agreed',
            'promise_reliability_outcome.completed_after_governed_review',
            'promise_reliability_outcome.valid_excused_exit',
            'promise_reliability_outcome.source_fact_corrected',
            'promise_reliability_outcome.review_required_boundary_intersection',
            'promise_reliability_outcome.source_scope_limited_after_review',
            'promise_reliability_outcome.freeze_or_narrowing_reversed_after_review'
        )
    ),
    writer_source_reference TEXT NOT NULL CHECK (char_length(trim(writer_source_reference)) > 0),
    promise_reference TEXT NOT NULL CHECK (char_length(trim(promise_reference)) > 0),
    realm_reference TEXT CHECK (
        realm_reference IS NULL
        OR char_length(trim(realm_reference)) > 0
    ),
    boundary_intersection_label TEXT CHECK (
        boundary_intersection_label IS NULL
        OR boundary_intersection_label IN (
            'consent',
            'block_mute_refusal_or_withdrawal',
            'age_assurance',
            'legal_hold',
            'critical_harm',
            'account_lifecycle',
            'appeal_correction_or_safety_review',
            'anti_abuse_suppression',
            'collusion_scam_or_coercion',
            'sensitive_exposure'
        )
    ),
    promise_terms_reference TEXT NOT NULL CHECK (char_length(trim(promise_terms_reference)) > 0),
    consent_at_formation_reference TEXT NOT NULL CHECK (
        char_length(trim(consent_at_formation_reference)) > 0
    ),
    consent_at_resolution_reference TEXT NOT NULL CHECK (
        char_length(trim(consent_at_resolution_reference)) > 0
    ),
    block_withdrawal_state_reference TEXT NOT NULL CHECK (
        char_length(trim(block_withdrawal_state_reference)) > 0
    ),
    age_assurance_state_reference TEXT NOT NULL CHECK (
        char_length(trim(age_assurance_state_reference)) > 0
    ),
    legal_hold_intersection_reference TEXT NOT NULL CHECK (
        char_length(trim(legal_hold_intersection_reference)) > 0
    ),
    critical_harm_case_reference TEXT NOT NULL CHECK (
        char_length(trim(critical_harm_case_reference)) > 0
    ),
    account_lifecycle_reference TEXT NOT NULL CHECK (
        char_length(trim(account_lifecycle_reference)) > 0
    ),
    anti_abuse_continuity_reference TEXT NOT NULL CHECK (
        char_length(trim(anti_abuse_continuity_reference)) > 0
    ),
    safety_case_reference TEXT NOT NULL CHECK (char_length(trim(safety_case_reference)) > 0),
    evidence_level_reference TEXT NOT NULL CHECK (char_length(trim(evidence_level_reference)) > 0),
    reason_fact_reference TEXT NOT NULL CHECK (char_length(trim(reason_fact_reference)) > 0),
    audit_reference TEXT NOT NULL CHECK (char_length(trim(audit_reference)) > 0),
    fact_idempotency_key TEXT NOT NULL CHECK (char_length(trim(fact_idempotency_key)) > 0),
    policy_version INTEGER NOT NULL CHECK (policy_version > 0),
    request_payload_hash TEXT NOT NULL CHECK (request_payload_hash ~ '^[0-9a-f]{64}$'),
    evidence_posture TEXT NOT NULL CHECK (evidence_posture IN ('bounded')),
    reviewability_posture TEXT NOT NULL CHECK (reviewability_posture IN ('reviewable')),
    retention_record_family TEXT NOT NULL CHECK (
        retention_record_family = 'Social Trust evidence or future Social Trust writer facts'
    ),
    retention_class_reference TEXT NOT NULL CHECK (
        retention_class_reference = 'R4 Trust / moderation / case'
    ),
    authority_posture TEXT NOT NULL CHECK (authority_posture IN ('writer_truth_only')),
    recorded_by_system TEXT NOT NULL DEFAULT 'social_trust_categorical_fact_persistence' CHECK (
        char_length(trim(recorded_by_system)) > 0
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        (
            source_fact_label = 'promise_reliability_outcome.review_required_boundary_intersection'
            AND boundary_intersection_label IS NOT NULL
        )
        OR (
            source_fact_label <> 'promise_reliability_outcome.review_required_boundary_intersection'
            AND boundary_intersection_label IS NULL
        )
    )
);

COMMENT ON TABLE social_trust.categorical_source_references IS
'C2 bounded Promise reliability accepted source references. These rows reference writer-owned source facts; they do not implement Promise runtime behavior or create source truth by themselves.';

COMMENT ON COLUMN social_trust.categorical_source_references.request_payload_hash IS
'SHA-256 hash of minimized C2 source-reference meaning used for deterministic duplicate-delivery drift checks. Raw PII and raw evidence do not belong here.';

CREATE UNIQUE INDEX IF NOT EXISTS social_trust_categorical_source_dedupe_unique
    ON social_trust.categorical_source_references (
        subject_account_id,
        policy_version,
        fact_idempotency_key
    );

COMMENT ON INDEX social_trust.social_trust_categorical_source_dedupe_unique IS
'Durable database-enforced idempotency boundary for C2 bounded Promise reliability categorical source references.';

CREATE INDEX IF NOT EXISTS social_trust_categorical_source_subject_created_idx
    ON social_trust.categorical_source_references (subject_account_id, created_at);

CREATE INDEX IF NOT EXISTS social_trust_categorical_source_retention_idx
    ON social_trust.categorical_source_references (
        retention_record_family,
        retention_class_reference
    );

CREATE TABLE IF NOT EXISTS social_trust.categorical_mutation_facts (
    mutation_fact_id UUID PRIMARY KEY,
    source_reference_id UUID NOT NULL UNIQUE REFERENCES social_trust.categorical_source_references(source_reference_id),
    subject_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    source_fact_label TEXT NOT NULL CHECK (
        source_fact_label IN (
            'promise_reliability_outcome.completed_as_agreed',
            'promise_reliability_outcome.completed_after_governed_review',
            'promise_reliability_outcome.valid_excused_exit',
            'promise_reliability_outcome.source_fact_corrected',
            'promise_reliability_outcome.review_required_boundary_intersection',
            'promise_reliability_outcome.source_scope_limited_after_review',
            'promise_reliability_outcome.freeze_or_narrowing_reversed_after_review'
        )
    ),
    mutation_fact_label TEXT NOT NULL CHECK (
        mutation_fact_label IN (
            'social_trust_mutation.bounded_promise_reliability_positive',
            'social_trust_mutation.no_effect_valid_excused_exit',
            'social_trust_mutation.bounded_promise_reliability_correction',
            'social_trust_mutation.bounded_promise_reliability_freeze',
            'social_trust_mutation.bounded_promise_reliability_narrowing',
            'social_trust_mutation.bounded_promise_reliability_recovery'
        )
    ),
    mutation_direction TEXT NOT NULL CHECK (
        mutation_direction IN (
            'positive',
            'no_effect',
            'correction',
            'freeze',
            'narrowing',
            'recovery'
        )
    ),
    mutation_magnitude TEXT NOT NULL CHECK (
        mutation_magnitude IN (
            'categorical',
            'no_effect',
            'forward_correction',
            'temporary_suppression',
            'scope_limited_restriction',
            'eligibility_restoration'
        )
    ),
    fact_idempotency_key TEXT NOT NULL CHECK (char_length(trim(fact_idempotency_key)) > 0),
    policy_version INTEGER NOT NULL CHECK (policy_version > 0),
    decision_payload_hash TEXT NOT NULL CHECK (decision_payload_hash ~ '^[0-9a-f]{64}$'),
    evidence_posture TEXT NOT NULL CHECK (evidence_posture IN ('bounded')),
    reviewability_posture TEXT NOT NULL CHECK (reviewability_posture IN ('reviewable')),
    retention_record_family TEXT NOT NULL CHECK (
        retention_record_family = 'Social Trust evidence or future Social Trust writer facts'
    ),
    retention_class_reference TEXT NOT NULL CHECK (
        retention_class_reference = 'R4 Trust / moderation / case'
    ),
    authority_posture TEXT NOT NULL CHECK (authority_posture IN ('writer_truth_only')),
    recorded_by_system TEXT NOT NULL DEFAULT 'social_trust_categorical_fact_persistence' CHECK (
        char_length(trim(recorded_by_system)) > 0
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        (
            source_fact_label IN (
                'promise_reliability_outcome.completed_as_agreed',
                'promise_reliability_outcome.completed_after_governed_review'
            )
            AND mutation_fact_label = 'social_trust_mutation.bounded_promise_reliability_positive'
            AND mutation_direction = 'positive'
            AND mutation_magnitude = 'categorical'
        )
        OR (
            source_fact_label = 'promise_reliability_outcome.valid_excused_exit'
            AND mutation_fact_label = 'social_trust_mutation.no_effect_valid_excused_exit'
            AND mutation_direction = 'no_effect'
            AND mutation_magnitude = 'no_effect'
        )
        OR (
            source_fact_label = 'promise_reliability_outcome.source_fact_corrected'
            AND mutation_fact_label = 'social_trust_mutation.bounded_promise_reliability_correction'
            AND mutation_direction = 'correction'
            AND mutation_magnitude = 'forward_correction'
        )
        OR (
            source_fact_label = 'promise_reliability_outcome.review_required_boundary_intersection'
            AND mutation_fact_label = 'social_trust_mutation.bounded_promise_reliability_freeze'
            AND mutation_direction = 'freeze'
            AND mutation_magnitude = 'temporary_suppression'
        )
        OR (
            source_fact_label = 'promise_reliability_outcome.source_scope_limited_after_review'
            AND mutation_fact_label = 'social_trust_mutation.bounded_promise_reliability_narrowing'
            AND mutation_direction = 'narrowing'
            AND mutation_magnitude = 'scope_limited_restriction'
        )
        OR (
            source_fact_label = 'promise_reliability_outcome.freeze_or_narrowing_reversed_after_review'
            AND mutation_fact_label = 'social_trust_mutation.bounded_promise_reliability_recovery'
            AND mutation_direction = 'recovery'
            AND mutation_magnitude = 'eligibility_restoration'
        )
    )
);

COMMENT ON TABLE social_trust.categorical_mutation_facts IS
'C2 bounded Promise reliability categorical Social Trust mutation facts. These facts are internal writer truth only and do not calculate scores, weights, ranks, public levels, display surfaces, Relationship Depth, discovery, recommendation, room, settlement, Promise runtime behavior, or projection refresh.';

COMMENT ON COLUMN social_trust.categorical_mutation_facts.decision_payload_hash IS
'SHA-256 hash of the minimized C2 categorical mutation decision for deterministic replay checks.';

CREATE UNIQUE INDEX IF NOT EXISTS social_trust_categorical_mutation_dedupe_unique
    ON social_trust.categorical_mutation_facts (
        subject_account_id,
        policy_version,
        fact_idempotency_key
    );

COMMENT ON INDEX social_trust.social_trust_categorical_mutation_dedupe_unique IS
'Durable database-enforced idempotency boundary for C2 bounded Promise reliability categorical mutation facts.';

CREATE INDEX IF NOT EXISTS social_trust_categorical_mutation_subject_created_idx
    ON social_trust.categorical_mutation_facts (subject_account_id, created_at);

CREATE INDEX IF NOT EXISTS social_trust_categorical_mutation_retention_idx
    ON social_trust.categorical_mutation_facts (
        retention_record_family,
        retention_class_reference
    );
