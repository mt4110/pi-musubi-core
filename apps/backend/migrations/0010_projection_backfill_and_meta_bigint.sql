ALTER TABLE projection.projection_meta
    ALTER COLUMN source_fact_count TYPE BIGINT,
    ALTER COLUMN projection_row_count TYPE BIGINT;

UPDATE projection.promise_views projection
SET realm_id = source.realm_id,
    initiator_account_id = source.initiator_account_id,
    counterparty_account_id = source.counterparty_account_id,
    current_intent_status = source.intent_status,
    deposit_amount_minor_units = source.deposit_amount_minor_units,
    currency_code = source.deposit_currency_code,
    deposit_scale = source.deposit_scale,
    latest_settlement_case_id = source.settlement_case_id,
    latest_settlement_status = source.settlement_status,
    source_watermark_at = source.source_watermark_at,
    source_fact_count = source.source_fact_count,
    freshness_checked_at = CURRENT_TIMESTAMP,
    projection_lag_ms = GREATEST(
        0::bigint,
        (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - source.source_watermark_at)) * 1000)::bigint
    ),
    last_projected_at = CURRENT_TIMESTAMP,
    rebuild_generation = NULL
FROM (
    SELECT
        promise.promise_intent_id,
        promise.realm_id,
        promise.initiator_account_id,
        promise.counterparty_account_id,
        promise.intent_status,
        promise.deposit_amount_minor_units,
        promise.deposit_currency_code,
        promise.deposit_scale,
        settlement.settlement_case_id,
        settlement.case_status AS settlement_status,
        GREATEST(
            promise.updated_at,
            COALESCE(settlement.updated_at, promise.updated_at)
        ) AS source_watermark_at,
        (
            1
            + CASE WHEN settlement.settlement_case_id IS NULL THEN 0 ELSE 1 END
        )::integer AS source_fact_count
    FROM dao.promise_intents promise
    LEFT JOIN dao.settlement_cases settlement
        ON settlement.promise_intent_id = promise.promise_intent_id
) source
WHERE projection.promise_intent_id = source.promise_intent_id;

UPDATE projection.settlement_views projection
SET realm_id = source.realm_id,
    promise_intent_id = source.promise_intent_id,
    latest_journal_entry_id = source.latest_journal_entry_id,
    current_settlement_status = source.case_status,
    total_funded_minor_units = source.total_funded_minor_units,
    currency_code = source.currency_code,
    source_watermark_at = source.source_watermark_at,
    source_fact_count = source.source_fact_count,
    freshness_checked_at = CURRENT_TIMESTAMP,
    projection_lag_ms = GREATEST(
        0::bigint,
        (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - source.source_watermark_at)) * 1000)::bigint
    ),
    proof_status = 'unavailable',
    proof_signal_count = 0,
    last_projected_at = CURRENT_TIMESTAMP,
    rebuild_generation = NULL
FROM (
    SELECT
        settlement.settlement_case_id,
        settlement.realm_id,
        settlement.promise_intent_id,
        settlement.case_status,
        latest_journal.latest_journal_entry_id,
        COALESCE(funded.total_funded_minor_units, 0) AS total_funded_minor_units,
        COALESCE(funded.currency_code, promise.deposit_currency_code, 'PI') AS currency_code,
        GREATEST(
            settlement.updated_at,
            promise.updated_at,
            COALESCE(journal_facts.latest_created_at, settlement.updated_at),
            COALESCE(receipt_facts.latest_updated_at, settlement.updated_at),
            COALESCE(observation_facts.latest_observed_at, settlement.updated_at)
        ) AS source_watermark_at,
        (
            2
            + COALESCE(journal_facts.journal_count, 0)
            + COALESCE(journal_facts.posting_count, 0)
            + COALESCE(receipt_facts.receipt_count, 0)
            + COALESCE(observation_facts.observation_count, 0)
        )::integer AS source_fact_count
    FROM dao.settlement_cases settlement
    JOIN dao.promise_intents promise
        ON promise.promise_intent_id = settlement.promise_intent_id
    LEFT JOIN LATERAL (
        SELECT journal_entry_id AS latest_journal_entry_id
        FROM ledger.journal_entries journal
        WHERE journal.settlement_case_id = settlement.settlement_case_id
        ORDER BY journal.created_at DESC, journal.journal_entry_id DESC
        LIMIT 1
    ) latest_journal ON TRUE
    LEFT JOIN LATERAL (
        SELECT
            count(*)::integer AS journal_count,
            max(created_at) AS latest_created_at,
            (
                SELECT count(*)::integer
                FROM ledger.account_postings posting
                JOIN ledger.journal_entries posting_journal
                    ON posting_journal.journal_entry_id = posting.journal_entry_id
                WHERE posting_journal.settlement_case_id = settlement.settlement_case_id
            ) AS posting_count
        FROM ledger.journal_entries journal
        WHERE journal.settlement_case_id = settlement.settlement_case_id
    ) journal_facts ON TRUE
    LEFT JOIN LATERAL (
        SELECT
            SUM(posting.amount_minor_units) AS total_funded_minor_units,
            max(posting.currency_code) AS currency_code
        FROM ledger.journal_entries journal
        JOIN ledger.account_postings posting
            ON posting.journal_entry_id = journal.journal_entry_id
        WHERE journal.settlement_case_id = settlement.settlement_case_id
          AND posting.ledger_account_code = 'user_secured_funds_liability'
          AND posting.direction = 'credit'
    ) funded ON TRUE
    LEFT JOIN LATERAL (
        SELECT
            count(*)::integer AS receipt_count,
            max(updated_at) AS latest_updated_at
        FROM core.payment_receipts receipt
        WHERE receipt.settlement_case_id = settlement.settlement_case_id
    ) receipt_facts ON TRUE
    LEFT JOIN LATERAL (
        SELECT
            count(*)::integer AS observation_count,
            max(observed_at) AS latest_observed_at
        FROM dao.settlement_observations observation
        WHERE observation.settlement_case_id = settlement.settlement_case_id
    ) observation_facts ON TRUE
) source
WHERE projection.settlement_case_id = source.settlement_case_id;
