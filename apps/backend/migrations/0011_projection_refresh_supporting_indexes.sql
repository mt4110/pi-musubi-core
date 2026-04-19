CREATE INDEX IF NOT EXISTS promise_intents_initiator_account_realm_updated_idx
    ON dao.promise_intents (initiator_account_id, realm_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS promise_intents_counterparty_account_realm_updated_idx
    ON dao.promise_intents (counterparty_account_id, realm_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS payment_receipts_promise_intent_updated_idx
    ON core.payment_receipts (promise_intent_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS payment_receipts_settlement_case_updated_idx
    ON core.payment_receipts (settlement_case_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS settlement_observations_case_observed_idx
    ON dao.settlement_observations (settlement_case_id, observed_at DESC);

CREATE INDEX IF NOT EXISTS journal_entries_settlement_case_created_idx
    ON ledger.journal_entries (settlement_case_id, created_at DESC);
