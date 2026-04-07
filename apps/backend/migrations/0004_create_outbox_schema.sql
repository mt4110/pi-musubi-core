CREATE SCHEMA IF NOT EXISTS outbox;

COMMENT ON SCHEMA outbox IS
'Coordination logs for delivery, retries, dedupe, and quarantine. This schema is not eternal business truth.';

CREATE TABLE IF NOT EXISTS outbox.events (
    event_id UUID PRIMARY KEY,
    idempotency_key UUID NOT NULL UNIQUE,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    payload_json JSONB NOT NULL,
    delivery_status TEXT NOT NULL CHECK (
        delivery_status IN ('pending', 'processing', 'published', 'failed', 'quarantined')
    ),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    published_at TIMESTAMPTZ,
    quarantined_at TIMESTAMPTZ,
    last_error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    causal_order BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE
);

COMMENT ON TABLE outbox.events IS
'Outbox payloads must stay pseudonymous. event_type plus schema_version define the durable contract, and causal_order provides writer-owned replay order. Raw PII does not belong here unless a later issue proves it is legally unavoidable.';

CREATE INDEX IF NOT EXISTS outbox_events_delivery_idx
    ON outbox.events (delivery_status, available_at);

CREATE TABLE IF NOT EXISTS outbox.consumer_inbox (
    inbox_entry_id UUID PRIMARY KEY,
    consumer_name TEXT NOT NULL,
    source_message_id UUID NOT NULL,
    payload_checksum TEXT,
    received_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at TIMESTAMPTZ,
    quarantined_at TIMESTAMPTZ,
    quarantine_reason TEXT,
    UNIQUE (consumer_name, source_message_id)
);

COMMENT ON TABLE outbox.consumer_inbox IS
'Consumer-side dedupe and quarantine log. This exists for idempotency discipline, not as a source of financial or profile truth.';
