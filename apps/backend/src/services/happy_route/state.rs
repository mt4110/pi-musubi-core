#![allow(dead_code)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use musubi_settlement_domain::{BackendPin, Money};

#[derive(Default)]
pub struct HappyRouteState {
    pub(super) accounts_by_id: HashMap<String, AccountRecord>,
    pub(super) account_id_by_pi_uid: HashMap<String, String>,
    pub(super) auth_sessions_by_token: HashMap<String, AuthSessionRecord>,
    pub(super) promise_intents_by_id: HashMap<String, PromiseIntentRecord>,
    pub(super) promise_intent_id_by_internal_idempotency_key: HashMap<(String, String), String>,
    pub(super) settlement_cases_by_id: HashMap<String, SettlementCaseRecord>,
    pub(super) settlement_case_id_by_promise_intent_id: HashMap<String, String>,
    pub(super) settlement_intents_by_id: HashMap<String, SettlementIntentRecord>,
    pub(super) settlement_submissions_by_id: HashMap<String, SettlementSubmissionRecord>,
    pub(super) settlement_submission_id_by_provider_submission_id: HashMap<String, String>,
    pub(super) raw_provider_callbacks_by_id: HashMap<String, RawProviderCallbackRecord>,
    pub(super) payment_receipts_by_id: HashMap<String, PaymentReceiptRecord>,
    pub(super) payment_receipt_id_by_business_key: HashMap<(String, String), String>,
    pub(super) settlement_observations: Vec<SettlementObservationRecord>,
    pub(super) ledger_journals_by_id: HashMap<String, LedgerJournalRecord>,
    pub(super) ledger_journal_order: Vec<String>,
    pub(super) ledger_postings: Vec<LedgerPostingRecord>,
    pub(super) outbox_messages_by_id: HashMap<String, OutboxMessageRecord>,
    pub(super) outbox_order: Vec<String>,
    pub(super) command_inbox_by_key: HashMap<(String, String), CommandInboxRecord>,
    pub(super) promise_views_by_id: HashMap<String, PromiseViewRecord>,
    pub(super) settlement_views_by_id: HashMap<String, SettlementViewRecord>,
}

#[derive(Clone, Debug)]
pub(super) struct AccountRecord {
    pub(super) account_id: String,
    pub(super) pi_uid: String,
    pub(super) username: String,
    pub(super) wallet_address: Option<String>,
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
pub(super) struct RawProviderCallbackRecord {
    pub(super) raw_callback_id: String,
    pub(super) payment_id: String,
    pub(super) payer_pi_uid: String,
    pub(super) amount: Money,
    pub(super) txid: Option<String>,
    pub(super) callback_status: String,
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
    pub(super) available_at: DateTime<Utc>,
    pub(super) published_at: Option<DateTime<Utc>>,
    pub(super) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(super) enum OutboxCommand {
    OpenHoldIntent { settlement_case_id: String },
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
