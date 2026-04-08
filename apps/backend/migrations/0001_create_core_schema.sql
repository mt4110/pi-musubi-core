CREATE SCHEMA IF NOT EXISTS core;

COMMENT ON SCHEMA core IS
'Mutable, legally governed records. Raw person-facing profile data belongs here, not in ledger or coordination schemas.';

CREATE TABLE IF NOT EXISTS core.accounts (
    account_id UUID PRIMARY KEY,
    account_class TEXT NOT NULL CHECK (
        account_class IN ('Ordinary Account', 'Controlled Exceptional Account')
    ),
    account_state TEXT NOT NULL CHECK (account_state IN ('active', 'suspended', 'deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMPTZ
);

COMMENT ON TABLE core.accounts IS
'Mutable account envelope for Ordinary Account and Controlled Exceptional Account records. Not a source of balance or trust truth.';

CREATE TABLE IF NOT EXISTS core.person_profiles (
    profile_id UUID PRIMARY KEY,
    account_id UUID NOT NULL UNIQUE REFERENCES core.accounts(account_id),
    display_name TEXT NOT NULL,
    birth_date DATE,
    profile_bio TEXT,
    locale_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMPTZ
);

COMMENT ON TABLE core.person_profiles IS
'Mutable person-facing profile data. Future raw PII stays in core so compliance and deletion do not pollute immutable truth.';
