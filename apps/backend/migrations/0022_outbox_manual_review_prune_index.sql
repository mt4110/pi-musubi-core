DROP INDEX IF EXISTS outbox.outbox_events_prune_idx;

CREATE INDEX IF NOT EXISTS outbox_events_prune_idx
    ON outbox.events (retain_until)
    WHERE delivery_status IN ('published', 'quarantined', 'manual_review')
      AND retain_until IS NOT NULL;

COMMENT ON INDEX outbox.outbox_events_prune_idx IS
'Supports archive-before-prune scans for terminal outbox coordination rows, including manual-review provider evidence.';
