ALTER TABLE projection.promise_views
    ADD COLUMN IF NOT EXISTS deposit_amount_minor_units BIGINT NOT NULL DEFAULT 0 CHECK (deposit_amount_minor_units >= 0),
    ADD COLUMN IF NOT EXISTS currency_code TEXT NOT NULL DEFAULT 'PI' CHECK (char_length(currency_code) >= 2),
    ADD COLUMN IF NOT EXISTS deposit_scale INTEGER NOT NULL DEFAULT 3 CHECK (deposit_scale >= 0),
    ADD COLUMN IF NOT EXISTS latest_settlement_status TEXT,
    ADD COLUMN IF NOT EXISTS source_watermark_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ADD COLUMN IF NOT EXISTS source_fact_count INTEGER NOT NULL DEFAULT 1 CHECK (source_fact_count >= 0),
    ADD COLUMN IF NOT EXISTS freshness_checked_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ADD COLUMN IF NOT EXISTS projection_lag_ms BIGINT NOT NULL DEFAULT 0 CHECK (projection_lag_ms >= 0),
    ADD COLUMN IF NOT EXISTS rebuild_generation UUID;

COMMENT ON COLUMN projection.promise_views.source_watermark_at IS
'Latest authoritative writer fact timestamp used to derive this Promise projection row.';

COMMENT ON COLUMN projection.promise_views.projection_lag_ms IS
'Derived lag between source_watermark_at and projection refresh time. Observability only; not business truth.';

ALTER TABLE projection.settlement_views
    ADD COLUMN IF NOT EXISTS source_watermark_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ADD COLUMN IF NOT EXISTS source_fact_count INTEGER NOT NULL DEFAULT 1 CHECK (source_fact_count >= 0),
    ADD COLUMN IF NOT EXISTS freshness_checked_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ADD COLUMN IF NOT EXISTS projection_lag_ms BIGINT NOT NULL DEFAULT 0 CHECK (projection_lag_ms >= 0),
    ADD COLUMN IF NOT EXISTS proof_status TEXT NOT NULL DEFAULT 'unavailable' CHECK (
        proof_status IN ('unavailable', 'available')
    ),
    ADD COLUMN IF NOT EXISTS proof_signal_count INTEGER NOT NULL DEFAULT 0 CHECK (proof_signal_count >= 0),
    ADD COLUMN IF NOT EXISTS rebuild_generation UUID;

COMMENT ON COLUMN projection.settlement_views.proof_status IS
'Proof persistence is outside #21/#22 writer truth. Settlement read models expose proof as unavailable until durable proof facts exist.';

CREATE TABLE IF NOT EXISTS projection.trust_snapshots (
    account_id UUID PRIMARY KEY,
    trust_posture TEXT NOT NULL CHECK (
        trust_posture IN (
            'insufficient_authoritative_facts',
            'bounded_reliability_observed',
            'review_attention_needed'
        )
    ),
    reason_codes JSONB NOT NULL DEFAULT '[]'::jsonb,
    promise_participation_count_90d INTEGER NOT NULL DEFAULT 0 CHECK (promise_participation_count_90d >= 0),
    funded_settlement_count_90d INTEGER NOT NULL DEFAULT 0 CHECK (funded_settlement_count_90d >= 0),
    manual_review_case_bucket TEXT NOT NULL DEFAULT 'none' CHECK (
        manual_review_case_bucket IN ('none', 'some', 'multiple')
    ),
    proof_status TEXT NOT NULL DEFAULT 'unavailable' CHECK (
        proof_status IN ('unavailable', 'available')
    ),
    proof_signal_count INTEGER NOT NULL DEFAULT 0 CHECK (proof_signal_count >= 0),
    source_watermark_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_fact_count INTEGER NOT NULL DEFAULT 0 CHECK (source_fact_count >= 0),
    freshness_checked_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    projection_lag_ms BIGINT NOT NULL DEFAULT 0 CHECK (projection_lag_ms >= 0),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    rebuild_generation UUID
);

COMMENT ON TABLE projection.trust_snapshots IS
'Coarse global trust projection derived from authoritative facts. It is not a score, rank, recommendation, or source of writer truth.';

COMMENT ON COLUMN projection.trust_snapshots.reason_codes IS
'Bounded reason codes only. Raw callbacks, raw evidence, private review details, and operator-only reasons do not belong here.';

CREATE TABLE IF NOT EXISTS projection.realm_trust_snapshots (
    account_id UUID NOT NULL,
    realm_id TEXT NOT NULL,
    trust_posture TEXT NOT NULL CHECK (
        trust_posture IN (
            'insufficient_authoritative_facts',
            'bounded_reliability_observed',
            'review_attention_needed'
        )
    ),
    reason_codes JSONB NOT NULL DEFAULT '[]'::jsonb,
    promise_participation_count_90d INTEGER NOT NULL DEFAULT 0 CHECK (promise_participation_count_90d >= 0),
    funded_settlement_count_90d INTEGER NOT NULL DEFAULT 0 CHECK (funded_settlement_count_90d >= 0),
    manual_review_case_bucket TEXT NOT NULL DEFAULT 'none' CHECK (
        manual_review_case_bucket IN ('none', 'some', 'multiple')
    ),
    proof_status TEXT NOT NULL DEFAULT 'unavailable' CHECK (
        proof_status IN ('unavailable', 'available')
    ),
    proof_signal_count INTEGER NOT NULL DEFAULT 0 CHECK (proof_signal_count >= 0),
    source_watermark_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_fact_count INTEGER NOT NULL DEFAULT 0 CHECK (source_fact_count >= 0),
    freshness_checked_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    projection_lag_ms BIGINT NOT NULL DEFAULT 0 CHECK (projection_lag_ms >= 0),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    rebuild_generation UUID,
    PRIMARY KEY (account_id, realm_id)
);

COMMENT ON TABLE projection.realm_trust_snapshots IS
'Realm-local trust projection. Realm-local facts stay scoped by realm_id and are not mixed into global display details.';

CREATE TABLE IF NOT EXISTS projection.projection_meta (
    projection_name TEXT PRIMARY KEY,
    last_rebuilt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_watermark_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_fact_count INTEGER NOT NULL DEFAULT 0 CHECK (source_fact_count >= 0),
    projection_row_count INTEGER NOT NULL DEFAULT 0 CHECK (projection_row_count >= 0),
    projection_lag_ms BIGINT NOT NULL DEFAULT 0 CHECK (projection_lag_ms >= 0),
    rebuild_generation UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE projection.projection_meta IS
'Projection rebuild and freshness metadata. This is observability for derived data, never writer-owned business truth.';
