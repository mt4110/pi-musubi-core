-- Design source: ISSUE-13-room-progression.md
-- GitHub issue number is intentionally not hardcoded.

CREATE TABLE IF NOT EXISTS dao.room_progression_tracks (
    room_progression_id UUID PRIMARY KEY,
    realm_id TEXT NOT NULL CHECK (char_length(trim(realm_id)) > 0),
    participant_a_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    participant_b_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    related_promise_intent_id UUID REFERENCES dao.promise_intents(promise_intent_id),
    related_settlement_case_id UUID REFERENCES dao.settlement_cases(settlement_case_id),
    current_stage TEXT NOT NULL CHECK (
        current_stage IN ('intent', 'coordination', 'relationship', 'sealed')
    ),
    current_status_code TEXT NOT NULL CHECK (
        current_status_code IN (
            'intent_open',
            'coordination_open',
            'relationship_open',
            'sealed_under_review',
            'sealed_restricted',
            'withdrawn',
            'blocked',
            'muted'
        )
    ),
    current_user_facing_reason_code TEXT NOT NULL CHECK (
        current_user_facing_reason_code IN (
            'room_created',
            'mutual_intent_acknowledged',
            'promise_draft_created',
            'bounded_coordination_accepted',
            'coordination_completed',
            'qualifying_promise_completed',
            'safety_review',
            'policy_review',
            'manual_hold_safety_review',
            'appeal_received',
            'proof_missing',
            'proof_inconclusive',
            'duplicate_or_invalid',
            'resolved_no_action',
            'restricted_after_review',
            'restored_after_review',
            'user_withdrew',
            'user_blocked',
            'user_muted'
        )
    ),
    current_review_case_id UUID REFERENCES dao.review_cases(review_case_id),
    source_fact_kind TEXT NOT NULL CHECK (char_length(trim(source_fact_kind)) > 0),
    source_fact_id TEXT NOT NULL CHECK (char_length(trim(source_fact_id)) > 0),
    source_snapshot_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    request_idempotency_key TEXT,
    request_payload_hash TEXT NOT NULL CHECK (request_payload_hash ~ '^[0-9a-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (participant_a_account_id <> participant_b_account_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS room_progression_tracks_idempotency_unique
    ON dao.room_progression_tracks (request_idempotency_key)
    WHERE request_idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS room_progression_tracks_participant_a_idx
    ON dao.room_progression_tracks (participant_a_account_id, updated_at);

CREATE INDEX IF NOT EXISTS room_progression_tracks_participant_b_idx
    ON dao.room_progression_tracks (participant_b_account_id, updated_at);

COMMENT ON TABLE dao.room_progression_tracks IS
'ISSUE-13 room progression writer envelope. It tracks current room posture from append-only progression facts without mutating Promise, settlement, or review truth.';

CREATE TABLE IF NOT EXISTS dao.room_progression_facts (
    room_progression_fact_id UUID PRIMARY KEY,
    room_progression_id UUID NOT NULL REFERENCES dao.room_progression_tracks(room_progression_id),
    from_stage TEXT NOT NULL CHECK (
        from_stage IN ('intent', 'coordination', 'relationship', 'sealed')
    ),
    to_stage TEXT NOT NULL CHECK (
        to_stage IN ('intent', 'coordination', 'relationship', 'sealed')
    ),
    transition_kind TEXT NOT NULL CHECK (
        transition_kind IN (
            'create',
            'advance_to_coordination',
            'advance_to_relationship',
            'seal',
            'restore',
            'mute',
            'block',
            'withdraw'
        )
    ),
    status_code TEXT NOT NULL CHECK (
        status_code IN (
            'intent_open',
            'coordination_open',
            'relationship_open',
            'sealed_under_review',
            'sealed_restricted',
            'withdrawn',
            'blocked',
            'muted'
        )
    ),
    user_facing_reason_code TEXT NOT NULL CHECK (
        user_facing_reason_code IN (
            'room_created',
            'mutual_intent_acknowledged',
            'promise_draft_created',
            'bounded_coordination_accepted',
            'coordination_completed',
            'qualifying_promise_completed',
            'safety_review',
            'policy_review',
            'manual_hold_safety_review',
            'appeal_received',
            'proof_missing',
            'proof_inconclusive',
            'duplicate_or_invalid',
            'resolved_no_action',
            'restricted_after_review',
            'restored_after_review',
            'user_withdrew',
            'user_blocked',
            'user_muted'
        )
    ),
    triggered_by_kind TEXT NOT NULL CHECK (
        triggered_by_kind IN ('system', 'participant', 'operator')
    ),
    triggered_by_account_id UUID REFERENCES core.accounts(account_id),
    source_fact_kind TEXT NOT NULL CHECK (char_length(trim(source_fact_kind)) > 0),
    source_fact_id TEXT NOT NULL CHECK (char_length(trim(source_fact_id)) > 0),
    source_snapshot_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    review_case_id UUID REFERENCES dao.review_cases(review_case_id),
    fact_idempotency_key TEXT,
    fact_payload_hash TEXT NOT NULL CHECK (fact_payload_hash ~ '^[0-9a-f]{64}$'),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS room_progression_facts_idempotency_unique
    ON dao.room_progression_facts (room_progression_id, fact_idempotency_key)
    WHERE fact_idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS room_progression_facts_track_idx
    ON dao.room_progression_facts (room_progression_id, recorded_at);

CREATE INDEX IF NOT EXISTS room_progression_facts_review_case_idx
    ON dao.room_progression_facts (review_case_id, recorded_at)
    WHERE review_case_id IS NOT NULL;

COMMENT ON TABLE dao.room_progression_facts IS
'Append-only ISSUE-13 room progression facts. Safety, block, mute, withdraw, seal, and restore actions add facts rather than rewriting Promise, settlement, or review truth.';

CREATE TABLE IF NOT EXISTS projection.room_progression_views (
    room_progression_id UUID PRIMARY KEY,
    realm_id TEXT NOT NULL,
    participant_a_account_id UUID NOT NULL,
    participant_b_account_id UUID NOT NULL,
    visible_stage TEXT NOT NULL CHECK (
        visible_stage IN ('intent', 'coordination', 'relationship', 'sealed')
    ),
    status_code TEXT NOT NULL CHECK (
        status_code IN (
            'intent_open',
            'coordination_open',
            'relationship_open',
            'sealed_under_review',
            'sealed_restricted',
            'withdrawn',
            'blocked',
            'muted'
        )
    ),
    user_facing_reason_code TEXT NOT NULL CHECK (
        user_facing_reason_code IN (
            'room_created',
            'mutual_intent_acknowledged',
            'promise_draft_created',
            'bounded_coordination_accepted',
            'coordination_completed',
            'qualifying_promise_completed',
            'safety_review',
            'policy_review',
            'manual_hold_safety_review',
            'appeal_received',
            'proof_missing',
            'proof_inconclusive',
            'duplicate_or_invalid',
            'resolved_no_action',
            'restricted_after_review',
            'restored_after_review',
            'user_withdrew',
            'user_blocked',
            'user_muted'
        )
    ),
    review_case_id UUID,
    review_pending BOOLEAN NOT NULL DEFAULT false,
    review_status TEXT,
    appeal_available BOOLEAN NOT NULL DEFAULT false,
    evidence_requested BOOLEAN NOT NULL DEFAULT false,
    source_watermark_at TIMESTAMPTZ NOT NULL,
    source_fact_count BIGINT NOT NULL CHECK (source_fact_count >= 0),
    projection_lag_ms BIGINT NOT NULL CHECK (projection_lag_ms >= 0),
    rebuild_generation BIGINT NOT NULL DEFAULT 1 CHECK (rebuild_generation > 0),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS room_progression_views_participant_a_idx
    ON projection.room_progression_views (participant_a_account_id, last_projected_at);

CREATE INDEX IF NOT EXISTS room_progression_views_participant_b_idx
    ON projection.room_progression_views (participant_b_account_id, last_projected_at);

COMMENT ON TABLE projection.room_progression_views IS
'Rebuildable ISSUE-13 participant-facing room progression read model. It exposes calm bounded status and reason codes, not raw evidence, internal notes, or operator identities.';
