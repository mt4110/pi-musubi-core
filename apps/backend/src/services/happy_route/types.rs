use super::state::{PromiseIntentRecord, SettlementCaseRecord};

#[derive(Debug)]
pub enum HappyRouteError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Internal(String),
}

impl HappyRouteError {
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::Unauthorized(message)
            | Self::NotFound(message)
            | Self::Internal(message) => message,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthenticationInput {
    pub pi_uid: String,
    pub username: String,
    pub wallet_address: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AuthenticatedAccount {
    pub token: String,
    pub account_id: String,
    pub pi_uid: String,
    pub username: String,
}

#[derive(Clone, Debug)]
pub struct PromiseIntentInput {
    pub internal_idempotency_key: String,
    pub realm_id: String,
    pub counterparty_account_id: String,
    pub deposit_amount_minor_units: i128,
    pub currency_code: String,
}

#[derive(Clone, Debug)]
pub struct PromiseIntentOutcome {
    pub promise_intent_id: String,
    pub settlement_case_id: String,
    pub case_status: String,
    pub outbox_event_ids: Vec<String>,
    pub replayed_intent: bool,
}

#[derive(Clone, Debug)]
pub struct PaymentCallbackInput {
    pub payment_id: String,
    pub payer_pi_uid: String,
    pub amount_minor_units: i128,
    pub currency_code: String,
    pub txid: Option<String>,
    pub callback_status: String,
}

#[derive(Clone, Debug)]
pub struct PaymentCallbackOutcome {
    pub payment_receipt_id: String,
    pub raw_callback_id: String,
    pub settlement_case_id: String,
    pub promise_intent_id: String,
    pub case_status: String,
    pub receipt_status: String,
    pub ledger_journal_id: Option<String>,
    pub outbox_event_ids: Vec<String>,
    pub duplicate_receipt: bool,
}

#[derive(Clone, Debug)]
pub struct DrainOutboxOutcome {
    pub processed_messages: Vec<ProcessedOutboxMessage>,
}

#[derive(Clone, Debug)]
pub struct ProcessedOutboxMessage {
    pub event_id: String,
    pub event_type: String,
    pub aggregate_id: String,
    pub consumer_name: String,
    pub provider_submission_id: Option<String>,
    pub already_processed: bool,
}

#[derive(Clone, Debug)]
pub struct SettlementViewSnapshot {
    pub settlement_case_id: String,
    pub promise_intent_id: String,
    pub realm_id: String,
    pub current_settlement_status: String,
    pub total_funded_minor_units: i128,
    pub currency_code: String,
    pub latest_journal_entry_id: Option<String>,
}

#[derive(Clone)]
pub(super) struct SubmissionPreparation {
    pub(super) settlement_case: SettlementCaseRecord,
    pub(super) promise_intent: PromiseIntentRecord,
    pub(super) settlement_intent_id: String,
    pub(super) settlement_submission_id: String,
    pub(super) internal_idempotency_key: String,
}

pub(super) enum OpenHoldIntentPrepareOutcome {
    ReplayNoop(ProcessedOutboxMessage),
    Ready(SubmissionPreparation),
}

pub(super) struct OpenHoldIntentPersistResult {
    pub(super) provider_submission_id: Option<String>,
}

#[derive(Clone)]
pub(super) struct CallbackContext {
    pub(super) raw_callback_id: String,
    pub(super) settlement_case: SettlementCaseRecord,
    pub(super) settlement_submission_id: String,
    pub(super) promise_intent: PromiseIntentRecord,
}

pub(super) fn processed_outbox_message(
    message: &super::state::OutboxMessageRecord,
    consumer_name: &str,
    provider_submission_id: Option<String>,
    already_processed: bool,
) -> ProcessedOutboxMessage {
    ProcessedOutboxMessage {
        event_id: message.event_id.clone(),
        event_type: message.event_type.clone(),
        aggregate_id: message.aggregate_id.clone(),
        consumer_name: consumer_name.to_owned(),
        provider_submission_id,
        already_processed,
    }
}
