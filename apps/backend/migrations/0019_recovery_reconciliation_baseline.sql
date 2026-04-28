CREATE TABLE IF NOT EXISTS outbox.recovery_runs (
    recovery_run_id UUID PRIMARY KEY,
    recovery_kind TEXT NOT NULL CHECK (
        recovery_kind IN ('orchestration_repair')
    ),
    triggered_by TEXT NOT NULL,
    dry_run BOOLEAN NOT NULL,
    request_reason TEXT NOT NULL,
    requested_by TEXT NOT NULL,
    max_rows_per_category INTEGER NOT NULL CHECK (max_rows_per_category > 0),
    include_stale_claims BOOLEAN NOT NULL,
    include_producer_cleanup BOOLEAN NOT NULL,
    include_callback_ingest BOOLEAN NOT NULL,
    include_verified_receipt_side_effects BOOLEAN NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ,
    stale_outbox_reclaimed_count BIGINT NOT NULL DEFAULT 0 CHECK (stale_outbox_reclaimed_count >= 0),
    stale_inbox_reclaimed_count BIGINT NOT NULL DEFAULT 0 CHECK (stale_inbox_reclaimed_count >= 0),
    producer_cleanup_repaired_count BIGINT NOT NULL DEFAULT 0 CHECK (producer_cleanup_repaired_count >= 0),
    callback_ingest_enqueued_count BIGINT NOT NULL DEFAULT 0 CHECK (callback_ingest_enqueued_count >= 0),
    verified_receipt_repaired_count BIGINT NOT NULL DEFAULT 0 CHECK (verified_receipt_repaired_count >= 0),
    limited BOOLEAN NOT NULL DEFAULT false,
    retain_until TIMESTAMPTZ NOT NULL DEFAULT (CURRENT_TIMESTAMP + interval '90 days'),
    notes JSONB NOT NULL DEFAULT '{}'::jsonb
);

COMMENT ON TABLE outbox.recovery_runs IS
'Internal recovery audit trail for writer-owned orchestration repair passes. This is coordination evidence, not business truth.';

CREATE INDEX IF NOT EXISTS outbox_events_processing_claim_idx
    ON outbox.events (claimed_until, last_attempt_at)
    WHERE delivery_status = 'processing';

CREATE INDEX IF NOT EXISTS command_inbox_processing_claim_idx
    ON outbox.command_inbox (claimed_until, received_at)
    WHERE status = 'processing';

CREATE INDEX IF NOT EXISTS recovery_runs_retain_until_idx
    ON outbox.recovery_runs (retain_until);

CREATE UNIQUE INDEX IF NOT EXISTS outbox_events_raw_callback_ingest_once_idx
    ON outbox.events (aggregate_id)
    WHERE aggregate_type = 'provider_callback'
      AND event_type = 'INGEST_PROVIDER_CALLBACK';

CREATE UNIQUE INDEX IF NOT EXISTS ledger_journal_receipt_recognized_once_idx
    ON ledger.journal_entries (settlement_case_id)
    WHERE entry_kind = 'receipt_recognized';
