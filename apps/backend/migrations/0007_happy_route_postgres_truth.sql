ALTER TABLE dao.promise_intents
    ALTER COLUMN realm_id TYPE TEXT USING realm_id::text,
    ADD COLUMN IF NOT EXISTS deposit_amount_minor_units BIGINT NOT NULL DEFAULT 0 CHECK (deposit_amount_minor_units >= 0),
    ADD COLUMN IF NOT EXISTS deposit_currency_code TEXT NOT NULL DEFAULT 'PI' CHECK (char_length(deposit_currency_code) >= 2),
    ADD COLUMN IF NOT EXISTS deposit_scale INTEGER NOT NULL DEFAULT 3 CHECK (deposit_scale >= 0);

ALTER TABLE dao.settlement_cases
    ALTER COLUMN realm_id TYPE TEXT USING realm_id::text,
    ADD COLUMN IF NOT EXISTS backend_key TEXT NOT NULL DEFAULT 'pi',
    ADD COLUMN IF NOT EXISTS backend_version TEXT NOT NULL DEFAULT 'sandbox-2026-04';

ALTER TABLE ledger.journal_entries
    ALTER COLUMN realm_id TYPE TEXT USING realm_id::text;

ALTER TABLE projection.promise_views
    ALTER COLUMN realm_id TYPE TEXT USING realm_id::text;

ALTER TABLE projection.settlement_views
    ALTER COLUMN realm_id TYPE TEXT USING realm_id::text;

CREATE TABLE IF NOT EXISTS core.pi_account_links (
    account_id UUID PRIMARY KEY REFERENCES core.accounts(account_id),
    pi_uid TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL,
    wallet_address TEXT,
    access_token_digest TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE core.pi_account_links IS
'Mutable Pi identity link for Day 1 sign-in. This stays in core, not ledger or coordination schemas.';

CREATE TABLE IF NOT EXISTS core.auth_sessions (
    session_token TEXT PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (account_id)
);

COMMENT ON TABLE core.auth_sessions IS
'Short-lived local auth session state. It is mutable core state and never settlement or ledger truth.';

CREATE TABLE IF NOT EXISTS dao.promise_intent_idempotency_keys (
    initiator_account_id UUID NOT NULL REFERENCES core.accounts(account_id),
    internal_idempotency_key TEXT NOT NULL,
    promise_intent_id UUID NOT NULL UNIQUE REFERENCES dao.promise_intents(promise_intent_id),
    request_payload_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (initiator_account_id, internal_idempotency_key)
);

COMMENT ON TABLE dao.promise_intent_idempotency_keys IS
'Durable internal idempotency boundary for Promise intent authoring. Replays must match request_payload_hash.';

CREATE TABLE IF NOT EXISTS dao.settlement_intents (
    settlement_intent_id UUID PRIMARY KEY,
    settlement_case_id UUID NOT NULL REFERENCES dao.settlement_cases(settlement_case_id),
    capability TEXT NOT NULL,
    internal_idempotency_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (settlement_case_id, internal_idempotency_key)
);

COMMENT ON TABLE dao.settlement_intents IS
'Durable settlement intent facts. Provider I/O is driven from these facts but does not define them.';

CREATE TABLE IF NOT EXISTS dao.settlement_submissions (
    settlement_submission_id UUID PRIMARY KEY,
    settlement_case_id UUID NOT NULL REFERENCES dao.settlement_cases(settlement_case_id),
    settlement_intent_id UUID NOT NULL REFERENCES dao.settlement_intents(settlement_intent_id),
    provider_submission_id TEXT UNIQUE,
    provider_ref TEXT,
    provider_idempotency_key TEXT,
    submission_status TEXT NOT NULL CHECK (
        submission_status IN ('pending', 'accepted', 'deferred', 'rejected', 'manual_review')
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (settlement_intent_id)
);

COMMENT ON TABLE dao.settlement_submissions IS
'Durable provider submission mapping owned by settlement control. provider_submission_id is the callback correlation boundary.';

CREATE TABLE IF NOT EXISTS dao.provider_attempts (
    provider_attempt_id UUID PRIMARY KEY,
    settlement_intent_id UUID NOT NULL REFERENCES dao.settlement_intents(settlement_intent_id),
    settlement_submission_id UUID NOT NULL REFERENCES dao.settlement_submissions(settlement_submission_id),
    provider_name TEXT NOT NULL,
    attempt_no INTEGER NOT NULL CHECK (attempt_no > 0),
    provider_request_key TEXT NOT NULL UNIQUE,
    provider_reference TEXT UNIQUE,
    provider_submission_id TEXT,
    request_hash TEXT NOT NULL,
    attempt_status TEXT NOT NULL,
    first_sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE dao.provider_attempts IS
'Durable provider-facing idempotency evidence. It is adapter evidence, not settlement finality.';

CREATE TABLE IF NOT EXISTS dao.settlement_observations (
    observation_id UUID PRIMARY KEY,
    settlement_case_id UUID NOT NULL REFERENCES dao.settlement_cases(settlement_case_id),
    settlement_submission_id UUID REFERENCES dao.settlement_submissions(settlement_submission_id),
    observation_kind TEXT NOT NULL,
    confidence TEXT NOT NULL,
    provider_ref TEXT,
    provider_tx_hash TEXT,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    observation_dedupe_key TEXT NOT NULL UNIQUE
);

COMMENT ON TABLE dao.settlement_observations IS
'Normalized provider observations. Providers are observed evidence; settlement and ledger tables remain the business truth.';

CREATE TABLE IF NOT EXISTS core.raw_provider_callback_dedupe (
    provider_name TEXT NOT NULL,
    dedupe_key TEXT NOT NULL,
    first_raw_callback_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (provider_name, dedupe_key)
);

COMMENT ON TABLE core.raw_provider_callback_dedupe IS
'Durable raw callback replay boundary. Duplicate raw callbacks still get evidence rows, but replay detection is database-enforced.';

CREATE TABLE IF NOT EXISTS core.raw_provider_callbacks (
    raw_callback_id UUID PRIMARY KEY,
    provider_name TEXT NOT NULL,
    dedupe_key TEXT NOT NULL,
    replay_of_raw_callback_id UUID,
    raw_body_bytes BYTEA NOT NULL,
    raw_body TEXT NOT NULL,
    redacted_headers JSONB NOT NULL,
    signature_valid BOOLEAN,
    provider_submission_id TEXT,
    provider_ref TEXT,
    payer_pi_uid TEXT,
    amount_minor_units BIGINT,
    currency_code TEXT,
    amount_scale INTEGER,
    txid TEXT,
    callback_status TEXT,
    received_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE core.raw_provider_callbacks IS
'Durable inbound provider callback evidence. HTTP callback intake writes this before any business progression.';

CREATE TABLE IF NOT EXISTS core.payment_receipts (
    payment_receipt_id UUID PRIMARY KEY,
    provider_key TEXT NOT NULL,
    external_payment_id TEXT NOT NULL,
    settlement_case_id UUID NOT NULL REFERENCES dao.settlement_cases(settlement_case_id),
    promise_intent_id UUID NOT NULL REFERENCES dao.promise_intents(promise_intent_id),
    amount_minor_units BIGINT NOT NULL CHECK (amount_minor_units >= 0),
    currency_code TEXT NOT NULL CHECK (char_length(currency_code) >= 2),
    amount_scale INTEGER NOT NULL CHECK (amount_scale >= 0),
    receipt_status TEXT NOT NULL CHECK (
        receipt_status IN ('verified', 'rejected', 'manual_review')
    ),
    raw_callback_id UUID NOT NULL REFERENCES core.raw_provider_callbacks(raw_callback_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (provider_key, external_payment_id)
);

COMMENT ON TABLE core.payment_receipts IS
'Idempotent MUSUBI payment receipt records derived from verified or reviewable callback evidence. The business key is provider_key plus external_payment_id.';

ALTER TABLE outbox.events
    DROP CONSTRAINT IF EXISTS outbox_events_delivery_status_check;

ALTER TABLE outbox.events
    ADD CONSTRAINT outbox_events_delivery_status_check CHECK (
        delivery_status IN ('pending', 'processing', 'published', 'failed', 'quarantined', 'manual_review')
    );

COMMENT ON CONSTRAINT outbox_events_delivery_status_check ON outbox.events IS
'manual_review is terminal coordination state for provider evidence that must be preserved for operator review.';
