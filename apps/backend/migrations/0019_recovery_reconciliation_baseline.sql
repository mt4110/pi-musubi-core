CREATE TABLE IF NOT EXISTS outbox.recovery_runs (
    recovery_run_id UUID PRIMARY KEY,
    recovery_kind TEXT NOT NULL CHECK (
        recovery_kind IN ('orchestration_repair')
    ),
    triggered_by TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ,
    stale_outbox_reclaimed_count INTEGER NOT NULL DEFAULT 0 CHECK (stale_outbox_reclaimed_count >= 0),
    stale_inbox_reclaimed_count INTEGER NOT NULL DEFAULT 0 CHECK (stale_inbox_reclaimed_count >= 0),
    producer_cleanup_repaired_count INTEGER NOT NULL DEFAULT 0 CHECK (producer_cleanup_repaired_count >= 0),
    callback_ingest_enqueued_count INTEGER NOT NULL DEFAULT 0 CHECK (callback_ingest_enqueued_count >= 0),
    verified_receipt_repaired_count INTEGER NOT NULL DEFAULT 0 CHECK (verified_receipt_repaired_count >= 0),
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
