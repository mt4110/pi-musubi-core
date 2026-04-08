CREATE SCHEMA IF NOT EXISTS ledger;

COMMENT ON SCHEMA ledger IS
'Append-only financial and accounting truth. No raw PII or mutable profile fields belong here.';

CREATE TABLE IF NOT EXISTS ledger.journal_entries (
    journal_entry_id UUID PRIMARY KEY,
    settlement_case_id UUID,
    promise_intent_id UUID,
    realm_id UUID NOT NULL,
    entry_kind TEXT NOT NULL,
    effective_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (settlement_case_id IS NOT NULL OR promise_intent_id IS NOT NULL)
);

COMMENT ON TABLE ledger.journal_entries IS
'Append-only journal header records. References stay pseudonymous so future deletion of core records does not require ledger rewrites.';

CREATE TABLE IF NOT EXISTS ledger.account_postings (
    posting_id UUID PRIMARY KEY,
    journal_entry_id UUID NOT NULL REFERENCES ledger.journal_entries(journal_entry_id),
    posting_order SMALLINT NOT NULL CHECK (posting_order > 0),
    ledger_account_code TEXT NOT NULL,
    account_id UUID,
    direction TEXT NOT NULL CHECK (direction IN ('debit', 'credit')),
    amount_minor_units BIGINT NOT NULL CHECK (amount_minor_units >= 0),
    currency_code TEXT NOT NULL CHECK (char_length(currency_code) >= 2),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (journal_entry_id, posting_order)
);

COMMENT ON TABLE ledger.account_postings IS
'Append-only postings stored in integer minor units. Pseudonymous account_id references may point to either Ordinary Account or Controlled Exceptional Account records; no names, bios, or other raw PII belong here.';
