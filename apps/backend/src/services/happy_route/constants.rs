pub(super) const PI_CURRENCY_CODE: &str = "PI";
pub(super) const PI_SCALE: u32 = 3;
pub(super) const PROVIDER_KEY: &str = "pi";
pub(super) const PROVIDER_VERSION: &str = "sandbox-2026-04";

pub(super) const OUTBOX_PENDING: &str = "pending";
pub(super) const OUTBOX_PROCESSING: &str = "processing";
pub(super) const OUTBOX_PUBLISHED: &str = "published";
pub(super) const OUTBOX_QUARANTINED: &str = "quarantined";
pub(super) const OUTBOX_MANUAL_REVIEW: &str = "manual_review";
pub(super) const OUTBOX_RETRY_BACKOFF_MILLIS: i64 = 250;
pub(super) const PROVIDER_CALLBACK_MAPPING_DEFER_ATTEMPTS: i32 = 12;
pub(super) const COMMAND_INBOX_RETENTION_MINUTES: i64 = 10;

pub(super) const PROMISE_INTENT_PROPOSED: &str = "proposed";
pub(super) const SETTLEMENT_CASE_PENDING_FUNDING: &str = "pending_funding";
pub(super) const SETTLEMENT_CASE_FUNDED: &str = "funded";

pub(super) const RECEIPT_STATUS_VERIFIED: &str = "verified";
pub(super) const RECEIPT_STATUS_REJECTED: &str = "rejected";
pub(super) const RECEIPT_STATUS_MANUAL_REVIEW: &str = "manual_review";

pub(super) const SETTLEMENT_ORCHESTRATOR: &str = "settlement-orchestrator";
pub(super) const PROVIDER_CALLBACK_CONSUMER: &str = "provider-callback-consumer";
pub(super) const PROJECTION_BUILDER: &str = "projection-builder";

pub(super) const EVENT_OPEN_HOLD_INTENT: &str = "OPEN_HOLD_INTENT";
pub(super) const EVENT_INGEST_PROVIDER_CALLBACK: &str = "INGEST_PROVIDER_CALLBACK";
pub(super) const EVENT_REFRESH_PROMISE_VIEW: &str = "REFRESH_PROMISE_VIEW";
pub(super) const EVENT_REFRESH_SETTLEMENT_VIEW: &str = "REFRESH_SETTLEMENT_VIEW";

pub(super) const LEDGER_ACCOUNT_PROVIDER_CLEARING_INBOUND: &str = "provider_clearing_inbound";
pub(super) const LEDGER_ACCOUNT_USER_SECURED_FUNDS_LIABILITY: &str = "user_secured_funds_liability";
pub(super) const LEDGER_DIRECTION_DEBIT: &str = "debit";
pub(super) const LEDGER_DIRECTION_CREDIT: &str = "credit";
