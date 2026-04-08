CREATE SCHEMA IF NOT EXISTS dao;

COMMENT ON SCHEMA dao IS
'Settlement and Promise coordination facts. This schema is not immutable ledger truth and not delivery infrastructure.';

CREATE TABLE IF NOT EXISTS dao.promise_intents (
    promise_intent_id UUID PRIMARY KEY,
    realm_id UUID NOT NULL,
    initiator_account_id UUID NOT NULL,
    counterparty_account_id UUID NOT NULL,
    intent_status TEXT NOT NULL CHECK (
        intent_status IN ('draft', 'proposed', 'accepted', 'withdrawn', 'expired')
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (initiator_account_id <> counterparty_account_id)
);

COMMENT ON TABLE dao.promise_intents IS
'Promise coordination facts keyed by pseudonymous UUIDs. No raw profile fields or balances belong here.';

CREATE TABLE IF NOT EXISTS dao.settlement_cases (
    settlement_case_id UUID PRIMARY KEY,
    promise_intent_id UUID NOT NULL UNIQUE REFERENCES dao.promise_intents(promise_intent_id),
    realm_id UUID NOT NULL,
    case_status TEXT NOT NULL CHECK (
        case_status IN ('pending_funding', 'funded', 'completed', 'cancelled')
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE dao.settlement_cases IS
'Settlement coordination records. Durable business truth lives in PostgreSQL, but immutable postings belong in ledger, not dao.';
