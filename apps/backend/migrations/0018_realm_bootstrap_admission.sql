-- Design source: ISSUE-15-realm-bootstrap-and-admission.md
-- GitHub issue number is intentionally not hardcoded.

CREATE TABLE IF NOT EXISTS dao.realm_requests (
    realm_request_id UUID PRIMARY KEY,
    requested_by_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    display_name TEXT NOT NULL CHECK (char_length(trim(display_name)) > 0),
    slug_candidate TEXT NOT NULL CHECK (char_length(trim(slug_candidate)) > 0),
    purpose_text TEXT NOT NULL CHECK (char_length(trim(purpose_text)) > 0),
    venue_context_json JSONB NOT NULL CHECK (
        jsonb_typeof(venue_context_json) = 'object' AND venue_context_json <> '{}'::jsonb
    ),
    expected_member_shape_json JSONB NOT NULL CHECK (
        jsonb_typeof(expected_member_shape_json) = 'object'
        AND expected_member_shape_json <> '{}'::jsonb
    ),
    bootstrap_rationale_text TEXT NOT NULL CHECK (char_length(trim(bootstrap_rationale_text)) > 0),
    proposed_sponsor_account_id UUID REFERENCES core.accounts(account_id),
    proposed_steward_account_id UUID REFERENCES core.accounts(account_id),
    request_state TEXT NOT NULL CHECK (
        request_state IN ('requested', 'pending_review', 'approved', 'rejected')
    ),
    review_reason_code TEXT NOT NULL CHECK (
        review_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    request_idempotency_key TEXT NOT NULL CHECK (
        char_length(trim(request_idempotency_key)) > 0
    ),
    request_payload_hash TEXT NOT NULL CHECK (request_payload_hash ~ '^[0-9a-f]{64}$'),
    reviewed_by_operator_id UUID REFERENCES core.accounts(account_id),
    review_decision_idempotency_key TEXT CHECK (
        review_decision_idempotency_key IS NULL
        OR char_length(trim(review_decision_idempotency_key)) > 0
    ),
    review_decision_payload_hash TEXT CHECK (
        review_decision_payload_hash IS NULL OR review_decision_payload_hash ~ '^[0-9a-f]{64}$'
    ),
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (
        (reviewed_at IS NULL AND reviewed_by_operator_id IS NULL)
        OR (reviewed_at IS NOT NULL AND reviewed_by_operator_id IS NOT NULL)
    ),
    CHECK (
        (
            request_state IN ('approved', 'rejected')
            AND review_decision_idempotency_key IS NOT NULL
            AND review_decision_payload_hash IS NOT NULL
        )
        OR (
            request_state IN ('requested', 'pending_review')
            AND review_decision_idempotency_key IS NULL
            AND review_decision_payload_hash IS NULL
        )
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS realm_requests_request_idempotency_unique
    ON dao.realm_requests (requested_by_account_id, request_idempotency_key);

CREATE UNIQUE INDEX IF NOT EXISTS realm_requests_open_slug_candidate_unique
    ON dao.realm_requests (slug_candidate)
    WHERE request_state IN ('requested', 'pending_review', 'approved');

CREATE INDEX IF NOT EXISTS realm_requests_state_created_idx
    ON dao.realm_requests (request_state, created_at);

COMMENT ON TABLE dao.realm_requests IS
'ISSUE-15 realm creation requests. Requests are reviewable writer truth and do not create public self-serve realm issuance by themselves.';

CREATE TABLE IF NOT EXISTS dao.realms (
    realm_id TEXT PRIMARY KEY CHECK (char_length(trim(realm_id)) > 0),
    slug TEXT NOT NULL UNIQUE CHECK (char_length(trim(slug)) > 0),
    display_name TEXT NOT NULL CHECK (char_length(trim(display_name)) > 0),
    realm_status TEXT NOT NULL CHECK (
        realm_status IN (
            'requested',
            'pending_review',
            'limited_bootstrap',
            'active',
            'restricted',
            'suspended'
        )
    ),
    public_reason_code TEXT NOT NULL CHECK (
        public_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    created_from_realm_request_id UUID NOT NULL REFERENCES dao.realm_requests(realm_request_id),
    steward_account_id UUID REFERENCES core.accounts(account_id),
    created_by_operator_id UUID NOT NULL REFERENCES core.accounts(account_id),
    restricted_at TIMESTAMPTZ,
    suspended_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS realms_status_updated_idx
    ON dao.realms (realm_status, updated_at);

CREATE UNIQUE INDEX IF NOT EXISTS realms_created_from_request_unique
    ON dao.realms (created_from_realm_request_id);

COMMENT ON TABLE dao.realms IS
'ISSUE-15 realm writer truth. Realms move through bounded bootstrap and restriction states; projection rows must not replace this truth.';

CREATE TABLE IF NOT EXISTS dao.realm_sponsor_records (
    realm_sponsor_record_id UUID PRIMARY KEY,
    realm_id TEXT NOT NULL REFERENCES dao.realms(realm_id),
    sponsor_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    sponsor_status TEXT NOT NULL CHECK (
        sponsor_status IN ('proposed', 'approved', 'active', 'rate_limited', 'revoked')
    ),
    quota_total BIGINT NOT NULL CHECK (quota_total > 0),
    status_reason_code TEXT NOT NULL CHECK (
        status_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    approved_by_operator_id UUID NOT NULL REFERENCES core.accounts(account_id),
    request_idempotency_key TEXT NOT NULL CHECK (
        char_length(trim(request_idempotency_key)) > 0
    ),
    request_payload_hash TEXT NOT NULL CHECK (request_payload_hash ~ '^[0-9a-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS realm_sponsor_records_idempotency_unique
    ON dao.realm_sponsor_records (realm_id, approved_by_operator_id, request_idempotency_key);

CREATE UNIQUE INDEX IF NOT EXISTS realm_sponsor_records_id_realm_unique
    ON dao.realm_sponsor_records (realm_sponsor_record_id, realm_id);

CREATE INDEX IF NOT EXISTS realm_sponsor_records_lookup_idx
    ON dao.realm_sponsor_records (sponsor_account_id, sponsor_status, realm_id);

COMMENT ON TABLE dao.realm_sponsor_records IS
'ISSUE-15 sponsor-backed bootstrap records. Sponsor authority is explicit, quota-bounded, reviewable, and revocable.';

CREATE TABLE IF NOT EXISTS dao.bootstrap_corridors (
    bootstrap_corridor_id UUID PRIMARY KEY,
    realm_id TEXT NOT NULL REFERENCES dao.realms(realm_id),
    corridor_status TEXT NOT NULL CHECK (
        corridor_status IN ('active', 'cooling_down', 'expired', 'disabled_by_operator')
    ),
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    member_cap BIGINT NOT NULL CHECK (member_cap > 0),
    sponsor_cap BIGINT NOT NULL CHECK (sponsor_cap > 0),
    review_threshold_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    disabled_reason_code TEXT CHECK (
        disabled_reason_code IS NULL OR disabled_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    created_by_operator_id UUID NOT NULL REFERENCES core.accounts(account_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (ends_at > starts_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS bootstrap_corridors_active_unique
    ON dao.bootstrap_corridors (realm_id)
    WHERE corridor_status IN ('active', 'cooling_down');

CREATE UNIQUE INDEX IF NOT EXISTS bootstrap_corridors_id_realm_unique
    ON dao.bootstrap_corridors (bootstrap_corridor_id, realm_id);

CREATE INDEX IF NOT EXISTS bootstrap_corridors_expiry_idx
    ON dao.bootstrap_corridors (corridor_status, ends_at, realm_id);

COMMENT ON TABLE dao.bootstrap_corridors IS
'ISSUE-15 bootstrap corridor flags. Corridors are temporary, bounded, observable, and enforced server-side.';

CREATE TABLE IF NOT EXISTS dao.realm_admissions (
    realm_admission_id UUID PRIMARY KEY,
    realm_id TEXT NOT NULL REFERENCES dao.realms(realm_id),
    account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    admission_kind TEXT NOT NULL CHECK (
        admission_kind IN ('normal', 'sponsor_backed', 'corridor', 'review_required')
    ),
    admission_status TEXT NOT NULL CHECK (
        admission_status IN ('pending', 'admitted', 'rejected', 'revoked')
    ),
    sponsor_record_id UUID,
    bootstrap_corridor_id UUID,
    granted_by_actor_kind TEXT NOT NULL CHECK (
        granted_by_actor_kind IN ('system', 'steward', 'operator')
    ),
    granted_by_actor_id UUID NOT NULL REFERENCES core.accounts(account_id),
    review_reason_code TEXT NOT NULL CHECK (
        review_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    source_fact_kind TEXT NOT NULL CHECK (char_length(trim(source_fact_kind)) > 0),
    source_fact_id TEXT NOT NULL CHECK (char_length(trim(source_fact_id)) > 0),
    source_snapshot_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    request_idempotency_key TEXT NOT NULL CHECK (
        char_length(trim(request_idempotency_key)) > 0
    ),
    request_payload_hash TEXT NOT NULL CHECK (request_payload_hash ~ '^[0-9a-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (sponsor_record_id, realm_id)
        REFERENCES dao.realm_sponsor_records(realm_sponsor_record_id, realm_id),
    FOREIGN KEY (bootstrap_corridor_id, realm_id)
        REFERENCES dao.bootstrap_corridors(bootstrap_corridor_id, realm_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS realm_admissions_idempotency_unique
    ON dao.realm_admissions (realm_id, granted_by_actor_id, request_idempotency_key);

CREATE UNIQUE INDEX IF NOT EXISTS realm_admissions_active_unique
    ON dao.realm_admissions (realm_id, account_id)
    WHERE admission_status IN ('pending', 'admitted');

CREATE INDEX IF NOT EXISTS realm_admissions_realm_status_idx
    ON dao.realm_admissions (realm_id, admission_status, created_at);

CREATE INDEX IF NOT EXISTS realm_admissions_account_latest_idx
    ON dao.realm_admissions (realm_id, account_id, updated_at DESC, created_at DESC, realm_admission_id DESC);

CREATE INDEX IF NOT EXISTS realm_admissions_sponsor_idx
    ON dao.realm_admissions (sponsor_record_id, admission_status)
    WHERE sponsor_record_id IS NOT NULL;

COMMENT ON TABLE dao.realm_admissions IS
'ISSUE-15 realm admissions. Admission kind is explicit and server-derived; UI state and projections must not create admission truth.';

CREATE TABLE IF NOT EXISTS dao.realm_review_triggers (
    realm_review_trigger_id UUID PRIMARY KEY,
    realm_id TEXT REFERENCES dao.realms(realm_id),
    trigger_kind TEXT NOT NULL CHECK (
        trigger_kind IN (
            'sponsor_concentration',
            'duplicate_venue_context',
            'suspicious_member_overlap',
            'proof_failure_rate',
            'safety_case_concentration',
            'operator_restriction',
            'quota_exceeded',
            'quota_abuse',
            'corridor_cap_pressure',
            'revoked_sponsor_lineage',
            'repeated_rejected_requests'
        )
    ),
    trigger_state TEXT NOT NULL CHECK (
        trigger_state IN ('open', 'acknowledged', 'resolved', 'suppressed')
    ),
    redacted_reason_code TEXT NOT NULL CHECK (
        redacted_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    related_account_id UUID REFERENCES core.accounts(account_id),
    related_realm_request_id UUID REFERENCES dao.realm_requests(realm_request_id),
    related_sponsor_record_id UUID REFERENCES dao.realm_sponsor_records(realm_sponsor_record_id),
    context_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    trigger_fingerprint TEXT NOT NULL CHECK (char_length(trim(trigger_fingerprint)) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS realm_review_triggers_open_unique
    ON dao.realm_review_triggers (trigger_fingerprint)
    WHERE trigger_state = 'open';

CREATE INDEX IF NOT EXISTS realm_review_triggers_queue_idx
    ON dao.realm_review_triggers (trigger_state, created_at, realm_id);

COMMENT ON TABLE dao.realm_review_triggers IS
'ISSUE-15 internal review signals for bootstrap/admission anomalies. These are operator-facing only and must not leak to participants.';

CREATE TABLE IF NOT EXISTS projection.realm_bootstrap_views (
    realm_id TEXT PRIMARY KEY REFERENCES dao.realms(realm_id),
    slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    realm_status TEXT NOT NULL CHECK (
        realm_status IN (
            'requested',
            'pending_review',
            'limited_bootstrap',
            'active',
            'restricted',
            'suspended'
        )
    ),
    admission_posture TEXT NOT NULL CHECK (
        admission_posture IN ('open', 'limited', 'review_required', 'closed')
    ),
    corridor_status TEXT NOT NULL CHECK (
        corridor_status IN ('none', 'active', 'cooling_down', 'expired', 'disabled_by_operator')
    ),
    public_reason_code TEXT NOT NULL CHECK (
        public_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    sponsor_display_state TEXT NOT NULL CHECK (
        sponsor_display_state IN ('none', 'sponsor_backed', 'steward_present', 'sponsor_and_steward')
    ),
    source_watermark_at TIMESTAMPTZ NOT NULL,
    source_fact_count BIGINT NOT NULL CHECK (source_fact_count >= 0),
    projection_lag_ms BIGINT NOT NULL CHECK (projection_lag_ms >= 0),
    rebuild_generation BIGINT NOT NULL DEFAULT 1 CHECK (rebuild_generation > 0),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS realm_bootstrap_views_status_idx
    ON projection.realm_bootstrap_views (realm_status, last_projected_at);

COMMENT ON TABLE projection.realm_bootstrap_views IS
'ISSUE-15 participant-safe realm bootstrap summary. It exposes calm bounded status without quota internals, raw notes, or review trigger detail.';

CREATE TABLE IF NOT EXISTS projection.realm_admission_views (
    realm_id TEXT NOT NULL REFERENCES dao.realms(realm_id),
    account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    admission_status TEXT NOT NULL CHECK (
        admission_status IN ('pending', 'admitted', 'rejected', 'revoked')
    ),
    admission_kind TEXT NOT NULL CHECK (
        admission_kind IN ('normal', 'sponsor_backed', 'corridor', 'review_required')
    ),
    public_reason_code TEXT NOT NULL CHECK (
        public_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    source_watermark_at TIMESTAMPTZ NOT NULL,
    source_fact_count BIGINT NOT NULL CHECK (source_fact_count >= 0),
    projection_lag_ms BIGINT NOT NULL CHECK (projection_lag_ms >= 0),
    rebuild_generation BIGINT NOT NULL DEFAULT 1 CHECK (rebuild_generation > 0),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (realm_id, account_id)
);

CREATE INDEX IF NOT EXISTS realm_admission_views_account_idx
    ON projection.realm_admission_views (account_id, last_projected_at);

COMMENT ON TABLE projection.realm_admission_views IS
'ISSUE-15 participant-safe admission summary per realm/account. It must not expose sponsor quota internals, operator identities, or review trigger detail.';

CREATE TABLE IF NOT EXISTS projection.realm_review_summaries (
    realm_id TEXT PRIMARY KEY REFERENCES dao.realms(realm_id),
    realm_status TEXT NOT NULL CHECK (
        realm_status IN (
            'requested',
            'pending_review',
            'limited_bootstrap',
            'active',
            'restricted',
            'suspended'
        )
    ),
    corridor_status TEXT NOT NULL CHECK (
        corridor_status IN ('none', 'active', 'cooling_down', 'expired', 'disabled_by_operator')
    ),
    corridor_remaining_seconds BIGINT NOT NULL CHECK (corridor_remaining_seconds >= 0),
    active_sponsor_count BIGINT NOT NULL CHECK (active_sponsor_count >= 0),
    sponsor_backed_admission_count BIGINT NOT NULL CHECK (sponsor_backed_admission_count >= 0),
    recent_admission_count_7d BIGINT NOT NULL CHECK (recent_admission_count_7d >= 0),
    open_review_trigger_count BIGINT NOT NULL CHECK (open_review_trigger_count >= 0),
    open_review_case_count BIGINT NOT NULL CHECK (open_review_case_count >= 0),
    latest_redacted_reason_code TEXT NOT NULL CHECK (
        latest_redacted_reason_code IN (
            'request_received',
            'review_required',
            'limited_bootstrap_active',
            'active_after_review',
            'request_rejected',
            'duplicate_or_invalid',
            'sponsor_required',
            'bootstrap_capacity_reached',
            'bootstrap_expired',
            'sponsor_rate_limited',
            'sponsor_revoked',
            'restricted_after_review',
            'suspended_after_review',
            'operator_restriction'
        )
    ),
    source_watermark_at TIMESTAMPTZ NOT NULL,
    source_fact_count BIGINT NOT NULL CHECK (source_fact_count >= 0),
    projection_lag_ms BIGINT NOT NULL CHECK (projection_lag_ms >= 0),
    rebuild_generation BIGINT NOT NULL DEFAULT 1 CHECK (rebuild_generation > 0),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS realm_review_summaries_status_idx
    ON projection.realm_review_summaries (realm_status, last_projected_at);

COMMENT ON TABLE projection.realm_review_summaries IS
'ISSUE-15 operator/steward bootstrap health summary. It is rebuildable and redacted, and must not become writer authority.';
