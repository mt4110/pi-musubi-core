ALTER TABLE outbox.events
    ADD COLUMN IF NOT EXISTS stream_key TEXT,
    ADD COLUMN IF NOT EXISTS payload_hash TEXT,
    ADD COLUMN IF NOT EXISTS claimed_by TEXT,
    ADD COLUMN IF NOT EXISTS claimed_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_attempt_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error_class TEXT,
    ADD COLUMN IF NOT EXISTS last_error_detail TEXT,
    ADD COLUMN IF NOT EXISTS quarantine_reason TEXT,
    ADD COLUMN IF NOT EXISTS retain_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS published_external_idempotency_key TEXT;

COMMENT ON COLUMN outbox.events.stream_key IS
'Ordering scope for replay and claim discipline. This is intentionally narrower than a global FIFO.';

COMMENT ON COLUMN outbox.events.payload_hash IS
'Deterministic payload hash used to audit envelope integrity and replay identity.';

COMMENT ON COLUMN outbox.events.retain_until IS
'Terminal coordination rows may be archived or pruned after this timestamp. This is not business truth retention.';

UPDATE outbox.events
SET stream_key = aggregate_type || ':' || aggregate_id::text
WHERE stream_key IS NULL;

UPDATE outbox.events
SET payload_hash = encode(sha256(convert_to(payload_json::text, 'UTF8')), 'hex')
WHERE payload_hash IS NULL;

ALTER TABLE outbox.events
    ALTER COLUMN stream_key SET NOT NULL,
    ALTER COLUMN payload_hash SET NOT NULL;

ALTER TABLE outbox.events
    DROP CONSTRAINT IF EXISTS outbox_events_last_error_class_check,
    DROP CONSTRAINT IF EXISTS outbox_events_quarantine_reason_check;

ALTER TABLE outbox.events
    ADD CONSTRAINT outbox_events_last_error_class_check CHECK (
        last_error_class IS NULL
        OR last_error_class IN ('transient', 'permanent', 'deferred')
    ),
    ADD CONSTRAINT outbox_events_quarantine_reason_check CHECK (
        quarantine_reason IS NULL
        OR quarantine_reason IN (
            'poison_pill',
            'permanent_failure',
            'attempt_budget_exceeded',
            'compatibility_window_expired'
        )
    );

CREATE TABLE IF NOT EXISTS outbox.outbox_attempts (
    event_id UUID NOT NULL REFERENCES outbox.events (event_id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    relay_name TEXT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL,
    failure_class TEXT,
    failure_code TEXT,
    failure_detail TEXT,
    external_idempotency_key TEXT,
    PRIMARY KEY (event_id, attempt_number)
);

COMMENT ON TABLE outbox.outbox_attempts IS
'Per-attempt delivery evidence for retries, quarantine, and reconciliation. This remains coordination data, not business truth.';

ALTER TABLE outbox.outbox_attempts
    DROP CONSTRAINT IF EXISTS outbox_attempts_failure_class_check;

ALTER TABLE outbox.outbox_attempts
    ADD CONSTRAINT outbox_attempts_failure_class_check CHECK (
        failure_class IS NULL
        OR failure_class IN ('transient', 'permanent', 'deferred')
    );

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_tables
        WHERE schemaname = 'outbox'
          AND tablename = 'consumer_inbox'
    ) AND NOT EXISTS (
        SELECT 1
        FROM pg_tables
        WHERE schemaname = 'outbox'
          AND tablename = 'command_inbox'
    ) THEN
        ALTER TABLE outbox.consumer_inbox RENAME TO command_inbox;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'outbox'
          AND table_name = 'command_inbox'
          AND column_name = 'source_message_id'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'outbox'
          AND table_name = 'command_inbox'
          AND column_name = 'source_event_id'
    ) THEN
        ALTER TABLE outbox.command_inbox RENAME COLUMN source_message_id TO source_event_id;
    END IF;
END $$;

ALTER TABLE outbox.command_inbox
    ADD COLUMN IF NOT EXISTS command_id UUID,
    ADD COLUMN IF NOT EXISTS source_event_id UUID,
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'processing' CHECK (
        status IN ('pending', 'processing', 'completed', 'quarantined')
    ),
    ADD COLUMN IF NOT EXISTS available_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    ADD COLUMN IF NOT EXISTS claimed_by TEXT,
    ADD COLUMN IF NOT EXISTS claimed_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error_class TEXT,
    ADD COLUMN IF NOT EXISTS last_error_code TEXT,
    ADD COLUMN IF NOT EXISTS last_error_detail TEXT,
    ADD COLUMN IF NOT EXISTS result_type TEXT,
    ADD COLUMN IF NOT EXISTS result_json JSONB,
    ADD COLUMN IF NOT EXISTS retain_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS command_type TEXT NOT NULL DEFAULT 'legacy.command',
    ADD COLUMN IF NOT EXISTS schema_version INTEGER NOT NULL DEFAULT 1 CHECK (schema_version > 0);

UPDATE outbox.command_inbox
SET source_event_id = COALESCE(source_event_id, inbox_entry_id)
WHERE source_event_id IS NULL;

UPDATE outbox.command_inbox
SET command_id = COALESCE(command_id, source_event_id)
WHERE command_id IS NULL;

ALTER TABLE outbox.command_inbox
    ALTER COLUMN source_event_id SET NOT NULL,
    ALTER COLUMN command_id SET NOT NULL;

ALTER TABLE outbox.command_inbox
    DROP CONSTRAINT IF EXISTS command_inbox_last_error_class_check,
    DROP CONSTRAINT IF EXISTS command_inbox_quarantine_reason_check;

ALTER TABLE outbox.command_inbox
    ADD CONSTRAINT command_inbox_last_error_class_check CHECK (
        last_error_class IS NULL
        OR last_error_class IN ('transient', 'permanent', 'deferred')
    ),
    ADD CONSTRAINT command_inbox_quarantine_reason_check CHECK (
        quarantine_reason IS NULL
        OR quarantine_reason IN (
            'poison_pill',
            'permanent_failure',
            'attempt_budget_exceeded',
            'compatibility_window_expired'
        )
    );

ALTER TABLE outbox.command_inbox
    DROP CONSTRAINT IF EXISTS consumer_inbox_consumer_name_source_message_id_key,
    DROP CONSTRAINT IF EXISTS command_inbox_consumer_name_source_message_id_key,
    DROP CONSTRAINT IF EXISTS command_inbox_consumer_name_source_event_id_key;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'outbox.command_inbox'::regclass
          AND conname = 'command_inbox_consumer_name_command_id_key'
    ) THEN
        ALTER TABLE outbox.command_inbox
            ADD CONSTRAINT command_inbox_consumer_name_command_id_key
            UNIQUE (consumer_name, command_id);
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS command_inbox_status_available_idx
    ON outbox.command_inbox (status, available_at);

CREATE INDEX IF NOT EXISTS outbox_events_prune_idx
    ON outbox.events (retain_until)
    WHERE delivery_status IN ('published', 'quarantined')
      AND retain_until IS NOT NULL;

CREATE INDEX IF NOT EXISTS command_inbox_prune_idx
    ON outbox.command_inbox (retain_until)
    WHERE status IN ('completed', 'quarantined')
      AND retain_until IS NOT NULL;

COMMENT ON TABLE outbox.command_inbox IS
'Durable consumer inbox boundary keyed by consumer_name plus command_id. source_event_id remains correlation evidence, not the dedupe key.';

CREATE TABLE IF NOT EXISTS outbox.outbox_event_archive (
    event_id UUID PRIMARY KEY,
    archived_at TIMESTAMPTZ NOT NULL,
    stream_key TEXT,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    payload_json JSONB NOT NULL,
    payload_hash TEXT,
    final_status TEXT NOT NULL,
    attempt_count INTEGER NOT NULL,
    causal_order BIGINT NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    quarantined_at TIMESTAMPTZ,
    last_attempt_at TIMESTAMPTZ,
    last_error_class TEXT,
    last_error_code TEXT,
    last_error_detail TEXT,
    quarantine_reason TEXT,
    retain_until TIMESTAMPTZ,
    published_external_idempotency_key TEXT
);

COMMENT ON TABLE outbox.outbox_event_archive IS
'Archive landing zone for pruned outbox rows. Replay order, correlation scope, and terminal diagnostics remain inspectable after hot-table pruning.';

CREATE TABLE IF NOT EXISTS outbox.outbox_attempt_archive (
    event_id UUID NOT NULL,
    attempt_number INTEGER NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL,
    relay_name TEXT NOT NULL,
    claimed_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL,
    failure_class TEXT,
    failure_code TEXT,
    failure_detail TEXT,
    external_idempotency_key TEXT,
    PRIMARY KEY (event_id, attempt_number)
);

COMMENT ON TABLE outbox.outbox_attempt_archive IS
'Archive landing zone for per-attempt delivery evidence. Retry and quarantine history survives pruning of the hot outbox tables.';

CREATE TABLE IF NOT EXISTS outbox.command_inbox_archive (
    consumer_name TEXT NOT NULL,
    command_id UUID NOT NULL,
    source_event_id UUID NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL,
    command_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    status TEXT NOT NULL,
    attempt_count INTEGER NOT NULL,
    payload_checksum TEXT,
    received_at TIMESTAMPTZ NOT NULL,
    processed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    last_error_class TEXT,
    last_error_code TEXT,
    last_error_detail TEXT,
    quarantine_reason TEXT,
    result_type TEXT,
    result_json JSONB,
    retain_until TIMESTAMPTZ,
    PRIMARY KEY (consumer_name, command_id)
);

COMMENT ON TABLE outbox.command_inbox_archive IS
'Archive landing zone for pruned command inbox rows. Completed and quarantined dedupe records stay inspectable after hot-table pruning.';
