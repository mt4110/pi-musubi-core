#![allow(dead_code)]

use chrono::{DateTime, Utc};
use musubi_settlement_domain::{BackendPin, Money};

#[derive(Clone, Debug)]
pub(super) struct AccountRecord {
    pub(super) account_id: String,
    pub(super) pi_uid: String,
    pub(super) username: String,
    pub(super) wallet_address: Option<String>,
    pub(super) access_token_digest: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct AuthSessionRecord {
    pub(super) token: String,
    pub(super) account_id: String,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct PromiseIntentRecord {
    pub(super) promise_intent_id: String,
    pub(super) internal_idempotency_key: String,
    pub(super) realm_id: String,
    pub(super) initiator_account_id: String,
    pub(super) counterparty_account_id: String,
    pub(super) deposit_amount: Money,
    pub(super) intent_status: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct SettlementCaseRecord {
    pub(super) settlement_case_id: String,
    pub(super) promise_intent_id: String,
    pub(super) realm_id: String,
    pub(super) case_status: String,
    pub(super) backend_pin: BackendPin,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct SettlementIntentRecord {
    pub(super) settlement_intent_id: String,
    pub(super) settlement_case_id: String,
    pub(super) capability: String,
    pub(super) internal_idempotency_key: String,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct SettlementSubmissionRecord {
    pub(super) settlement_submission_id: String,
    pub(super) settlement_case_id: String,
    pub(super) settlement_intent_id: String,
    pub(super) provider_submission_id: Option<String>,
    pub(super) provider_ref: Option<String>,
    pub(super) provider_idempotency_key: String,
    pub(super) submission_status: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct ProviderAttemptRecord {
    pub(super) provider_attempt_id: String,
    pub(super) settlement_intent_id: String,
    pub(super) settlement_submission_id: String,
    pub(super) provider_name: String,
    pub(super) attempt_no: i32,
    pub(super) provider_request_key: String,
    pub(super) provider_reference: Option<String>,
    pub(super) provider_submission_id: Option<String>,
    pub(super) request_hash: String,
    pub(super) attempt_status: String,
    pub(super) first_sent_at: DateTime<Utc>,
    pub(super) last_observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct RawProviderCallbackRecord {
    pub(super) raw_callback_id: String,
    pub(super) provider_name: String,
    pub(super) dedupe_key: String,
    pub(super) replay_of_raw_callback_id: Option<String>,
    pub(super) raw_body_bytes: Vec<u8>,
    pub(super) raw_body: String,
    pub(super) redacted_headers: Vec<(String, String)>,
    pub(super) signature_valid: Option<bool>,
    pub(super) provider_submission_id: Option<String>,
    pub(super) provider_ref: Option<String>,
    pub(super) payer_pi_uid: Option<String>,
    pub(super) amount_minor_units: Option<i128>,
    pub(super) currency_code: Option<String>,
    pub(super) amount: Option<Money>,
    pub(super) txid: Option<String>,
    pub(super) callback_status: Option<String>,
    pub(super) received_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct PaymentReceiptRecord {
    pub(super) payment_receipt_id: String,
    pub(super) provider_key: String,
    pub(super) external_payment_id: String,
    pub(super) settlement_case_id: String,
    pub(super) promise_intent_id: String,
    pub(super) amount: Money,
    pub(super) receipt_status: String,
    pub(super) raw_callback_id: String,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct SettlementObservationRecord {
    pub(super) observation_id: String,
    pub(super) settlement_case_id: String,
    pub(super) settlement_submission_id: Option<String>,
    pub(super) observation_kind: String,
    pub(super) confidence: String,
    pub(super) provider_ref: Option<String>,
    pub(super) provider_tx_hash: Option<String>,
    pub(super) observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct LedgerJournalRecord {
    pub(super) journal_entry_id: String,
    pub(super) settlement_case_id: String,
    pub(super) promise_intent_id: String,
    pub(super) realm_id: String,
    pub(super) entry_kind: String,
    pub(super) effective_at: DateTime<Utc>,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct LedgerPostingRecord {
    pub(super) posting_id: String,
    pub(super) journal_entry_id: String,
    pub(super) posting_order: i16,
    pub(super) ledger_account_code: String,
    pub(super) account_id: Option<String>,
    pub(super) direction: String,
    pub(super) amount: Money,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct OutboxMessageRecord {
    pub(super) event_id: String,
    pub(super) idempotency_key: String,
    pub(super) aggregate_type: String,
    pub(super) aggregate_id: String,
    pub(super) event_type: String,
    pub(super) schema_version: i32,
    pub(super) command: OutboxCommand,
    pub(super) delivery_status: String,
    pub(super) attempt_count: i32,
    pub(super) last_error_class: Option<String>,
    pub(super) last_error_message: Option<String>,
    pub(super) available_at: DateTime<Utc>,
    pub(super) published_at: Option<DateTime<Utc>>,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) enum OutboxCommand {
    OpenHoldIntent { settlement_case_id: String },
    IngestProviderCallback { raw_callback_id: String },
    RefreshPromiseView { promise_intent_id: String },
    RefreshSettlementView { settlement_case_id: String },
}

#[derive(Clone, Debug)]
pub(super) struct CommandInboxRecord {
    pub(super) consumer_name: String,
    pub(super) source_message_id: String,
    pub(super) received_at: DateTime<Utc>,
    pub(super) processed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub(super) struct PromiseViewRecord {
    pub(super) promise_intent_id: String,
    pub(super) realm_id: String,
    pub(super) initiator_account_id: String,
    pub(super) counterparty_account_id: String,
    pub(super) current_intent_status: String,
    pub(super) latest_settlement_case_id: Option<String>,
    pub(super) last_projected_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) struct SettlementViewRecord {
    pub(super) settlement_case_id: String,
    pub(super) realm_id: String,
    pub(super) promise_intent_id: String,
    pub(super) latest_journal_entry_id: Option<String>,
    pub(super) current_settlement_status: String,
    pub(super) total_funded: Money,
    pub(super) last_projected_at: DateTime<Utc>,
}
