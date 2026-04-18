ALTER TABLE projection.promise_views
    ALTER COLUMN source_fact_count TYPE BIGINT;

ALTER TABLE projection.settlement_views
    ALTER COLUMN source_fact_count TYPE BIGINT;

ALTER TABLE projection.trust_snapshots
    ALTER COLUMN promise_participation_count_90d TYPE BIGINT,
    ALTER COLUMN funded_settlement_count_90d TYPE BIGINT,
    ALTER COLUMN source_fact_count TYPE BIGINT;

ALTER TABLE projection.realm_trust_snapshots
    ALTER COLUMN promise_participation_count_90d TYPE BIGINT,
    ALTER COLUMN funded_settlement_count_90d TYPE BIGINT,
    ALTER COLUMN source_fact_count TYPE BIGINT;
