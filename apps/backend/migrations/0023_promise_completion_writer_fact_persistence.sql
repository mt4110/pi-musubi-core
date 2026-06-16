CREATE SCHEMA IF NOT EXISTS promise_completion;

COMMENT ON SCHEMA promise_completion IS
'Writer-owned Promise completion fact persistence. This schema does not contain projection truth, Social Trust mutations, Relationship Depth facts, settlement movement, provider payloads, or raw Personal Data.';

CREATE TABLE IF NOT EXISTS promise_completion.writer_fact_records (
    writer_fact_id UUID PRIMARY KEY,
    promise_reference TEXT NOT NULL CHECK (char_length(trim(promise_reference)) > 0),
    realm_id TEXT NOT NULL CHECK (char_length(trim(realm_id)) > 0),
    fact_family TEXT NOT NULL CHECK (
        fact_family IN (
            'source_route_candidate',
            'completion_state_transition',
            'completion_outcome_reference',
            'correction_or_supersession',
            'access_audit_retention_support'
        )
    ),
    source_route_class TEXT NOT NULL CHECK (
        source_route_class IN (
            'mutual_accountable_completion_acknowledgement',
            'governed_review_completion'
        )
    ),
    previous_completion_state_class TEXT CHECK (
        previous_completion_state_class IS NULL
        OR previous_completion_state_class IN (
            'completion_unavailable',
            'completion_pending_mutual_acknowledgement',
            'completion_review_required',
            'completion_under_governed_review',
            'completion_accepted',
            'completion_rejected',
            'completion_expired',
            'completion_corrected_or_superseded',
            'completion_closed'
        )
    ),
    completion_state_class TEXT NOT NULL CHECK (
        completion_state_class IN (
            'completion_unavailable',
            'completion_pending_mutual_acknowledgement',
            'completion_review_required',
            'completion_under_governed_review',
            'completion_accepted',
            'completion_rejected',
            'completion_expired',
            'completion_corrected_or_superseded',
            'completion_closed'
        )
    ),
    completed_reference_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    promise_terms_reference TEXT NOT NULL CHECK (
        char_length(trim(promise_terms_reference)) > 0
    ),
    participant_set_reference TEXT NOT NULL CHECK (
        char_length(trim(participant_set_reference)) > 0
    ),
    ordinary_participant_acknowledgement_reference TEXT CHECK (
        ordinary_participant_acknowledgement_reference IS NULL
        OR char_length(trim(ordinary_participant_acknowledgement_reference)) > 0
    ),
    governed_review_reference TEXT CHECK (
        governed_review_reference IS NULL
        OR char_length(trim(governed_review_reference)) > 0
    ),
    review_authority_reference TEXT CHECK (
        review_authority_reference IS NULL
        OR char_length(trim(review_authority_reference)) > 0
    ),
    proof_eligibility_reference TEXT CHECK (
        proof_eligibility_reference IS NULL
        OR char_length(trim(proof_eligibility_reference)) > 0
    ),
    proof_evidence_writer_fact_reference TEXT CHECK (
        proof_evidence_writer_fact_reference IS NULL
        OR char_length(trim(proof_evidence_writer_fact_reference)) > 0
    ),
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
    reason_code_class TEXT NOT NULL CHECK (char_length(trim(reason_code_class)) > 0),
    evidence_level_reference TEXT NOT NULL CHECK (
        char_length(trim(evidence_level_reference)) > 0
    ),
    correction_or_supersession_reference TEXT CHECK (
        correction_or_supersession_reference IS NULL
        OR char_length(trim(correction_or_supersession_reference)) > 0
    ),
    prior_writer_fact_id UUID REFERENCES promise_completion.writer_fact_records(writer_fact_id),
    policy_version INTEGER NOT NULL CHECK (policy_version > 0),
    fact_idempotency_key TEXT NOT NULL CHECK (
        char_length(trim(fact_idempotency_key)) > 0
    ),
    request_payload_hash TEXT NOT NULL CHECK (request_payload_hash ~ '^[0-9a-f]{64}$'),
    decision_payload_hash TEXT NOT NULL CHECK (decision_payload_hash ~ '^[0-9a-f]{64}$'),
    retention_class_reference TEXT NOT NULL CHECK (
        char_length(trim(retention_class_reference)) > 0
    ),
    access_audit_reference TEXT NOT NULL CHECK (
        char_length(trim(access_audit_reference)) > 0
    ),
    projection_non_authority_posture TEXT NOT NULL CHECK (
        projection_non_authority_posture = 'projection_non_authoritative'
    ),
    authority_posture TEXT NOT NULL CHECK (authority_posture = 'writer_truth_only'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        completed_reference_eligible = FALSE
        OR completion_state_class = 'completion_accepted'
    ),
    CHECK (
        source_route_class <> 'mutual_accountable_completion_acknowledgement'
        OR ordinary_participant_acknowledgement_reference IS NOT NULL
    ),
    CHECK (
        source_route_class <> 'governed_review_completion'
        OR (
            governed_review_reference IS NOT NULL
            AND review_authority_reference IS NOT NULL
        )
    ),
    CHECK (
        (
            proof_eligibility_reference IS NULL
            AND proof_evidence_writer_fact_reference IS NULL
        )
        OR (
            proof_eligibility_reference IS NOT NULL
            AND proof_evidence_writer_fact_reference IS NOT NULL
        )
    ),
    CHECK (
        fact_family <> 'correction_or_supersession'
        OR (
            correction_or_supersession_reference IS NOT NULL
            OR prior_writer_fact_id IS NOT NULL
        )
    )
);

COMMENT ON TABLE promise_completion.writer_fact_records IS
'Append-only Promise completion writer fact envelopes. These records preserve writer truth references only and do not run Promise completion runtime behavior.';

COMMENT ON COLUMN promise_completion.writer_fact_records.completed_reference_eligible IS
'Participant-safe completed reference eligibility. Database constraint limits true to completion_accepted.';

COMMENT ON COLUMN promise_completion.writer_fact_records.request_payload_hash IS
'SHA-256 hash of minimized writer fact meaning used for deterministic duplicate-delivery drift checks. Raw evidence, raw provider payloads, and raw PII do not belong here.';

CREATE OR REPLACE FUNCTION promise_completion.reject_writer_fact_record_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'promise_completion.writer_fact_records is append-only';
END;
$$;

COMMENT ON FUNCTION promise_completion.reject_writer_fact_record_mutation() IS
'Rejects UPDATE and DELETE against Promise completion writer fact records. Corrections must be new facts.';

DROP TRIGGER IF EXISTS promise_completion_writer_fact_records_no_update
    ON promise_completion.writer_fact_records;

CREATE TRIGGER promise_completion_writer_fact_records_no_update
    BEFORE UPDATE OR DELETE ON promise_completion.writer_fact_records
    FOR EACH ROW
    EXECUTE FUNCTION promise_completion.reject_writer_fact_record_mutation();

CREATE UNIQUE INDEX IF NOT EXISTS promise_completion_writer_fact_dedupe_unique
    ON promise_completion.writer_fact_records (
        realm_id,
        promise_reference,
        policy_version,
        fact_idempotency_key
    );

COMMENT ON INDEX promise_completion.promise_completion_writer_fact_dedupe_unique IS
'Durable database-enforced idempotency boundary for Promise completion writer fact envelopes.';

CREATE INDEX IF NOT EXISTS promise_completion_writer_fact_promise_created_idx
    ON promise_completion.writer_fact_records (promise_reference, created_at);

CREATE INDEX IF NOT EXISTS promise_completion_writer_fact_realm_created_idx
    ON promise_completion.writer_fact_records (realm_id, created_at);

CREATE INDEX IF NOT EXISTS promise_completion_writer_fact_retention_idx
    ON promise_completion.writer_fact_records (retention_class_reference, created_at);

CREATE INDEX IF NOT EXISTS promise_completion_writer_fact_prior_idx
    ON promise_completion.writer_fact_records (prior_writer_fact_id)
    WHERE prior_writer_fact_id IS NOT NULL;
