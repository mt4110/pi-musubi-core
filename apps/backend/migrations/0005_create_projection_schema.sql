CREATE SCHEMA IF NOT EXISTS projection;

COMMENT ON SCHEMA projection IS
'Derivable read models only. Projection data is rebuildable and never the authoritative writer source.';

CREATE TABLE IF NOT EXISTS projection.promise_views (
    promise_intent_id UUID PRIMARY KEY,
    realm_id UUID NOT NULL,
    initiator_account_id UUID NOT NULL,
    counterparty_account_id UUID NOT NULL,
    current_intent_status TEXT NOT NULL,
    latest_settlement_case_id UUID,
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE projection.promise_views IS
'Rebuildable Promise read model derived from dao and ledger facts. It exists for reads, not settlement decisions.';

CREATE TABLE IF NOT EXISTS projection.settlement_views (
    settlement_case_id UUID PRIMARY KEY,
    realm_id UUID NOT NULL,
    promise_intent_id UUID NOT NULL,
    latest_journal_entry_id UUID,
    current_settlement_status TEXT NOT NULL,
    total_funded_minor_units BIGINT NOT NULL DEFAULT 0 CHECK (total_funded_minor_units >= 0),
    currency_code TEXT NOT NULL CHECK (char_length(currency_code) = 3),
    last_projected_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE projection.settlement_views IS
'Rebuildable settlement read model. Values here summarize authoritative truth but must never replace dao or ledger as the writer source.';
