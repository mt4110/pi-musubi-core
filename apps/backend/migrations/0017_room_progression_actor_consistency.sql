ALTER TABLE dao.room_progression_facts
    ADD CONSTRAINT room_progression_facts_triggered_by_actor_consistency_chk
    CHECK (
        (triggered_by_kind = 'system' AND triggered_by_account_id IS NULL)
        OR (
            triggered_by_kind IN ('participant', 'operator')
            AND triggered_by_account_id IS NOT NULL
        )
    );
