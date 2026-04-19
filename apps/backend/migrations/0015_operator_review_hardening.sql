-- Design source: ISSUE-12-operator-review-appeal-evidence.md
-- GitHub issue number is intentionally not hardcoded.

ALTER TABLE dao.review_cases
    ADD COLUMN IF NOT EXISTS request_payload_hash TEXT
        CHECK (request_payload_hash IS NULL OR char_length(request_payload_hash) = 64);

ALTER TABLE dao.operator_decision_facts
    ADD COLUMN IF NOT EXISTS decision_payload_hash TEXT
        CHECK (decision_payload_hash IS NULL OR char_length(decision_payload_hash) = 64);

ALTER TABLE dao.appeal_cases
    ADD COLUMN IF NOT EXISTS appeal_payload_hash TEXT
        CHECK (appeal_payload_hash IS NULL OR char_length(appeal_payload_hash) = 64);

COMMENT ON COLUMN dao.review_cases.request_payload_hash IS
'ISSUE-12 idempotency payload hash for normal replay comparison. Legacy NULL values are backfilled from preserved review-case fields on first compatible replay.';

COMMENT ON COLUMN dao.operator_decision_facts.decision_payload_hash IS
'ISSUE-12 idempotency payload hash for normal replay comparison. Legacy NULL values are backfilled from preserved decision fact fields on first compatible replay.';

COMMENT ON COLUMN dao.appeal_cases.appeal_payload_hash IS
'ISSUE-12 idempotency payload hash for normal replay comparison. Legacy NULL values are backfilled from preserved appeal fields on first compatible replay.';
