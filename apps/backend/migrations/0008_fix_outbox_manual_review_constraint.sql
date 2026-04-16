ALTER TABLE outbox.events
    DROP CONSTRAINT IF EXISTS events_delivery_status_check,
    DROP CONSTRAINT IF EXISTS outbox_events_delivery_status_check;

ALTER TABLE outbox.events
    ADD CONSTRAINT outbox_events_delivery_status_check CHECK (
        delivery_status IN ('pending', 'processing', 'published', 'failed', 'quarantined', 'manual_review')
    );

COMMENT ON CONSTRAINT outbox_events_delivery_status_check ON outbox.events IS
'manual_review is terminal coordination state for provider evidence that must be preserved for operator review.';
