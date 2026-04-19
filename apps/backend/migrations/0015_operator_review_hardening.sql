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
'ISSUE-12 idempotency payload hash. Replays compare this hash instead of reloading potentially sensitive source snapshots.';

COMMENT ON COLUMN dao.operator_decision_facts.decision_payload_hash IS
'ISSUE-12 idempotency payload hash. Replays compare this hash instead of reloading internal operator notes or decision payload JSON.';

COMMENT ON COLUMN dao.appeal_cases.appeal_payload_hash IS
'ISSUE-12 idempotency payload hash. Replays compare this hash instead of reloading appellant statements or new evidence summaries.';
