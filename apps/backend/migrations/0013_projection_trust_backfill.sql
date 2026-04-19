INSERT INTO projection.trust_snapshots (
    account_id,
    trust_posture,
    reason_codes,
    promise_participation_count_90d,
    funded_settlement_count_90d,
    manual_review_case_bucket,
    proof_status,
    proof_signal_count,
    source_watermark_at,
    source_fact_count,
    freshness_checked_at,
    projection_lag_ms,
    last_projected_at,
    rebuild_generation
)
WITH participant_accounts AS (
    SELECT DISTINCT account_id
    FROM (
        SELECT initiator_account_id AS account_id
        FROM dao.promise_intents
        UNION ALL
        SELECT counterparty_account_id AS account_id
        FROM dao.promise_intents
    ) participants
),
account_promises AS (
    SELECT
        account.account_id,
        promise.promise_intent_id,
        promise.updated_at AS promise_updated_at,
        settlement.settlement_case_id,
        settlement.case_status,
        settlement.updated_at AS settlement_updated_at
    FROM participant_accounts account
    JOIN dao.promise_intents promise
        ON promise.initiator_account_id = account.account_id
        OR promise.counterparty_account_id = account.account_id
    LEFT JOIN dao.settlement_cases settlement
        ON settlement.promise_intent_id = promise.promise_intent_id
),
account_promise_counts AS (
    SELECT account_id, count(*)::bigint AS promise_count
    FROM account_promises
    GROUP BY account_id
),
receipt_facts AS (
    SELECT
        promise.account_id,
        count(*)::bigint AS receipt_count,
        count(*) FILTER (WHERE receipt.receipt_status = 'manual_review')::bigint AS manual_review_count,
        max(receipt.updated_at) AS latest_receipt_at
    FROM account_promises promise
    JOIN core.payment_receipts receipt
        ON receipt.promise_intent_id = promise.promise_intent_id
    GROUP BY promise.account_id
),
observation_facts AS (
    SELECT
        promise.account_id,
        count(*)::bigint AS observation_count,
        max(observation.observed_at) AS latest_observation_at
    FROM account_promises promise
    JOIN dao.settlement_observations observation
        ON observation.settlement_case_id = promise.settlement_case_id
    GROUP BY promise.account_id
),
facts AS (
    SELECT
        account_id,
        count(*) FILTER (
            WHERE promise_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
        )::bigint AS promise_participation_count_90d,
        count(*) FILTER (
            WHERE case_status = 'funded'
              AND settlement_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
        )::bigint AS funded_settlement_count_90d,
        count(settlement_case_id)::bigint AS settlement_count,
        max(promise_updated_at) AS latest_promise_at,
        max(settlement_updated_at) AS latest_settlement_at
    FROM account_promises
    GROUP BY account_id
),
source AS (
    SELECT
        account.account_id,
        COALESCE(facts.promise_participation_count_90d, 0)::bigint AS promise_participation_count_90d,
        COALESCE(facts.funded_settlement_count_90d, 0)::bigint AS funded_settlement_count_90d,
        COALESCE(receipt_facts.manual_review_count, 0)::bigint AS manual_review_count,
        (
            COALESCE(account_promise_counts.promise_count, 0)
            + COALESCE(facts.settlement_count, 0)
            + COALESCE(receipt_facts.receipt_count, 0)
            + COALESCE(observation_facts.observation_count, 0)
        )::bigint AS source_fact_count,
        GREATEST(
            COALESCE(facts.latest_promise_at, 'epoch'::timestamptz),
            COALESCE(facts.latest_settlement_at, 'epoch'::timestamptz),
            COALESCE(receipt_facts.latest_receipt_at, 'epoch'::timestamptz),
            COALESCE(observation_facts.latest_observation_at, 'epoch'::timestamptz)
        ) AS source_watermark_at
    FROM participant_accounts account
    LEFT JOIN account_promise_counts
        ON account_promise_counts.account_id = account.account_id
    LEFT JOIN facts
        ON facts.account_id = account.account_id
    LEFT JOIN receipt_facts
        ON receipt_facts.account_id = account.account_id
    LEFT JOIN observation_facts
        ON observation_facts.account_id = account.account_id
),
shaped AS (
    SELECT
        account_id,
        CASE
            WHEN manual_review_count > 0 THEN 'review_attention_needed'
            WHEN funded_settlement_count_90d > 0 THEN 'bounded_reliability_observed'
            ELSE 'insufficient_authoritative_facts'
        END AS trust_posture,
        (
            SELECT COALESCE(jsonb_agg(code ORDER BY code), '[]'::jsonb)
            FROM (
                VALUES
                    ('deposit_backed_promise_funded', funded_settlement_count_90d > 0),
                    ('manual_review_bucket_nonzero', manual_review_count > 0),
                    ('promise_participation_observed', promise_participation_count_90d > 0),
                    ('proof_unavailable', TRUE)
            ) AS reasons(code, include_reason)
            WHERE include_reason
        ) AS reason_codes,
        promise_participation_count_90d,
        funded_settlement_count_90d,
        CASE
            WHEN manual_review_count = 0 THEN 'none'
            WHEN manual_review_count <= 2 THEN 'some'
            ELSE 'multiple'
        END AS manual_review_case_bucket,
        CASE
            WHEN source_watermark_at = 'epoch'::timestamptz THEN CURRENT_TIMESTAMP
            ELSE source_watermark_at
        END AS source_watermark_at,
        source_fact_count
    FROM source
)
SELECT
    shaped.account_id,
    shaped.trust_posture,
    shaped.reason_codes,
    shaped.promise_participation_count_90d,
    shaped.funded_settlement_count_90d,
    shaped.manual_review_case_bucket,
    'unavailable',
    0,
    shaped.source_watermark_at,
    shaped.source_fact_count,
    CURRENT_TIMESTAMP,
    GREATEST(
        0::bigint,
        (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - shaped.source_watermark_at)) * 1000)::bigint
    ),
    CURRENT_TIMESTAMP,
    NULL
FROM shaped
ON CONFLICT (account_id) DO UPDATE
SET trust_posture = EXCLUDED.trust_posture,
    reason_codes = EXCLUDED.reason_codes,
    promise_participation_count_90d = EXCLUDED.promise_participation_count_90d,
    funded_settlement_count_90d = EXCLUDED.funded_settlement_count_90d,
    manual_review_case_bucket = EXCLUDED.manual_review_case_bucket,
    proof_status = EXCLUDED.proof_status,
    proof_signal_count = EXCLUDED.proof_signal_count,
    source_watermark_at = EXCLUDED.source_watermark_at,
    source_fact_count = EXCLUDED.source_fact_count,
    freshness_checked_at = EXCLUDED.freshness_checked_at,
    projection_lag_ms = EXCLUDED.projection_lag_ms,
    last_projected_at = EXCLUDED.last_projected_at,
    rebuild_generation = EXCLUDED.rebuild_generation;

INSERT INTO projection.realm_trust_snapshots (
    account_id,
    realm_id,
    trust_posture,
    reason_codes,
    promise_participation_count_90d,
    funded_settlement_count_90d,
    manual_review_case_bucket,
    proof_status,
    proof_signal_count,
    source_watermark_at,
    source_fact_count,
    freshness_checked_at,
    projection_lag_ms,
    last_projected_at,
    rebuild_generation
)
WITH participant_account_realms AS (
    SELECT DISTINCT account_id, realm_id
    FROM (
        SELECT initiator_account_id AS account_id, realm_id
        FROM dao.promise_intents
        UNION ALL
        SELECT counterparty_account_id AS account_id, realm_id
        FROM dao.promise_intents
    ) participants
),
account_promises AS (
    SELECT
        participant.account_id,
        participant.realm_id,
        promise.promise_intent_id,
        promise.updated_at AS promise_updated_at,
        settlement.settlement_case_id,
        settlement.case_status,
        settlement.updated_at AS settlement_updated_at
    FROM participant_account_realms participant
    JOIN dao.promise_intents promise
        ON promise.realm_id = participant.realm_id
       AND (
            promise.initiator_account_id = participant.account_id
            OR promise.counterparty_account_id = participant.account_id
       )
    LEFT JOIN dao.settlement_cases settlement
        ON settlement.promise_intent_id = promise.promise_intent_id
),
account_promise_counts AS (
    SELECT account_id, realm_id, count(*)::bigint AS promise_count
    FROM account_promises
    GROUP BY account_id, realm_id
),
receipt_facts AS (
    SELECT
        promise.account_id,
        promise.realm_id,
        count(*)::bigint AS receipt_count,
        count(*) FILTER (WHERE receipt.receipt_status = 'manual_review')::bigint AS manual_review_count,
        max(receipt.updated_at) AS latest_receipt_at
    FROM account_promises promise
    JOIN core.payment_receipts receipt
        ON receipt.promise_intent_id = promise.promise_intent_id
    GROUP BY promise.account_id, promise.realm_id
),
observation_facts AS (
    SELECT
        promise.account_id,
        promise.realm_id,
        count(*)::bigint AS observation_count,
        max(observation.observed_at) AS latest_observation_at
    FROM account_promises promise
    JOIN dao.settlement_observations observation
        ON observation.settlement_case_id = promise.settlement_case_id
    GROUP BY promise.account_id, promise.realm_id
),
facts AS (
    SELECT
        account_id,
        realm_id,
        count(*) FILTER (
            WHERE promise_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
        )::bigint AS promise_participation_count_90d,
        count(*) FILTER (
            WHERE case_status = 'funded'
              AND settlement_updated_at >= CURRENT_TIMESTAMP - interval '90 days'
        )::bigint AS funded_settlement_count_90d,
        count(settlement_case_id)::bigint AS settlement_count,
        max(promise_updated_at) AS latest_promise_at,
        max(settlement_updated_at) AS latest_settlement_at
    FROM account_promises
    GROUP BY account_id, realm_id
),
source AS (
    SELECT
        participant.account_id,
        participant.realm_id,
        COALESCE(facts.promise_participation_count_90d, 0)::bigint AS promise_participation_count_90d,
        COALESCE(facts.funded_settlement_count_90d, 0)::bigint AS funded_settlement_count_90d,
        COALESCE(receipt_facts.manual_review_count, 0)::bigint AS manual_review_count,
        (
            COALESCE(account_promise_counts.promise_count, 0)
            + COALESCE(facts.settlement_count, 0)
            + COALESCE(receipt_facts.receipt_count, 0)
            + COALESCE(observation_facts.observation_count, 0)
        )::bigint AS source_fact_count,
        GREATEST(
            COALESCE(facts.latest_promise_at, 'epoch'::timestamptz),
            COALESCE(facts.latest_settlement_at, 'epoch'::timestamptz),
            COALESCE(receipt_facts.latest_receipt_at, 'epoch'::timestamptz),
            COALESCE(observation_facts.latest_observation_at, 'epoch'::timestamptz)
        ) AS source_watermark_at
    FROM participant_account_realms participant
    LEFT JOIN account_promise_counts
        ON account_promise_counts.account_id = participant.account_id
       AND account_promise_counts.realm_id = participant.realm_id
    LEFT JOIN facts
        ON facts.account_id = participant.account_id
       AND facts.realm_id = participant.realm_id
    LEFT JOIN receipt_facts
        ON receipt_facts.account_id = participant.account_id
       AND receipt_facts.realm_id = participant.realm_id
    LEFT JOIN observation_facts
        ON observation_facts.account_id = participant.account_id
       AND observation_facts.realm_id = participant.realm_id
),
shaped AS (
    SELECT
        account_id,
        realm_id,
        CASE
            WHEN manual_review_count > 0 THEN 'review_attention_needed'
            WHEN funded_settlement_count_90d > 0 THEN 'bounded_reliability_observed'
            ELSE 'insufficient_authoritative_facts'
        END AS trust_posture,
        (
            SELECT COALESCE(jsonb_agg(code ORDER BY code), '[]'::jsonb)
            FROM (
                VALUES
                    ('deposit_backed_promise_funded', funded_settlement_count_90d > 0),
                    ('manual_review_bucket_nonzero', manual_review_count > 0),
                    ('promise_participation_observed', promise_participation_count_90d > 0),
                    ('proof_unavailable', TRUE),
                    ('realm_scoped', TRUE)
            ) AS reasons(code, include_reason)
            WHERE include_reason
        ) AS reason_codes,
        promise_participation_count_90d,
        funded_settlement_count_90d,
        CASE
            WHEN manual_review_count = 0 THEN 'none'
            WHEN manual_review_count <= 2 THEN 'some'
            ELSE 'multiple'
        END AS manual_review_case_bucket,
        CASE
            WHEN source_watermark_at = 'epoch'::timestamptz THEN CURRENT_TIMESTAMP
            ELSE source_watermark_at
        END AS source_watermark_at,
        source_fact_count
    FROM source
)
SELECT
    shaped.account_id,
    shaped.realm_id,
    shaped.trust_posture,
    shaped.reason_codes,
    shaped.promise_participation_count_90d,
    shaped.funded_settlement_count_90d,
    shaped.manual_review_case_bucket,
    'unavailable',
    0,
    shaped.source_watermark_at,
    shaped.source_fact_count,
    CURRENT_TIMESTAMP,
    GREATEST(
        0::bigint,
        (EXTRACT(EPOCH FROM (CURRENT_TIMESTAMP - shaped.source_watermark_at)) * 1000)::bigint
    ),
    CURRENT_TIMESTAMP,
    NULL
FROM shaped
ON CONFLICT (account_id, realm_id) DO UPDATE
SET trust_posture = EXCLUDED.trust_posture,
    reason_codes = EXCLUDED.reason_codes,
    promise_participation_count_90d = EXCLUDED.promise_participation_count_90d,
    funded_settlement_count_90d = EXCLUDED.funded_settlement_count_90d,
    manual_review_case_bucket = EXCLUDED.manual_review_case_bucket,
    proof_status = EXCLUDED.proof_status,
    proof_signal_count = EXCLUDED.proof_signal_count,
    source_watermark_at = EXCLUDED.source_watermark_at,
    source_fact_count = EXCLUDED.source_fact_count,
    freshness_checked_at = EXCLUDED.freshness_checked_at,
    projection_lag_ms = EXCLUDED.projection_lag_ms,
    last_projected_at = EXCLUDED.last_projected_at,
    rebuild_generation = EXCLUDED.rebuild_generation;
