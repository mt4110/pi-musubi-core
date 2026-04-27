use chrono::{DateTime, Utc};

use super::state::{PromiseIntentRecord, SettlementCaseRecord};

#[derive(Debug)]
pub enum HappyRouteError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    ProviderCallbackMappingDeferred(String),
    Database {
        message: String,
        code: Option<String>,
        constraint: Option<String>,
        retryable: bool,
    },
    Provider {
        class: ProviderErrorClass,
        message: String,
    },
    Internal(String),
}

impl HappyRouteError {
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::Unauthorized(message)
            | Self::NotFound(message)
            | Self::ProviderCallbackMappingDeferred(message)
            | Self::Database { message, .. }
            | Self::Provider { message, .. }
            | Self::Internal(message) => message,
        }
    }

    pub(super) fn provider_error_class(&self) -> Option<ProviderErrorClass> {
        match self {
            Self::ProviderCallbackMappingDeferred(_) => Some(ProviderErrorClass::Retryable),
            Self::Database { retryable, .. } => Some(if *retryable {
                ProviderErrorClass::Retryable
            } else {
                ProviderErrorClass::Terminal
            }),
            Self::Provider { class, .. } => Some(*class),
            _ => None,
        }
    }

    pub(super) fn is_provider_callback_mapping_deferred(&self) -> bool {
        matches!(self, Self::ProviderCallbackMappingDeferred(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderErrorClass {
    Retryable,
    Terminal,
    ManualReview,
}

#[derive(Clone, Debug)]
pub struct AuthenticationInput {
    pub pi_uid: String,
    pub username: String,
    pub wallet_address: Option<String>,
    pub access_token: String,
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
    pub raw_body_bytes: Vec<u8>,
    pub redacted_headers: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub(super) struct ParsedPaymentCallback {
    pub provider_submission_id: String,
    pub payer_pi_uid: String,
    pub amount_minor_units: i128,
    pub currency_code: String,
}

#[derive(Clone, Debug)]
pub(super) struct RawPaymentCallbackFields {
    pub provider_submission_id: Option<String>,
    pub payer_pi_uid: Option<String>,
    pub amount_minor_units: Option<i128>,
    pub currency_code: Option<String>,
    pub txid: Option<String>,
    pub callback_status: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PaymentCallbackAccepted {
    pub raw_callback_id: String,
    pub duplicate_callback: bool,
    pub outbox_event_ids: Vec<String>,
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

#[derive(Clone, Debug)]
pub struct ProjectionProvenance {
    pub source_watermark_at: DateTime<Utc>,
    pub source_fact_count: i64,
    pub freshness_checked_at: DateTime<Utc>,
    pub projection_lag_ms: i64,
    pub last_projected_at: DateTime<Utc>,
    pub rebuild_generation: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PromiseProjectionSnapshot {
    pub promise_intent_id: String,
    pub realm_id: String,
    pub initiator_account_id: String,
    pub counterparty_account_id: String,
    pub current_intent_status: String,
    pub deposit_amount_minor_units: i128,
    pub currency_code: String,
    pub deposit_scale: i32,
    pub latest_settlement_case_id: Option<String>,
    pub latest_settlement_status: Option<String>,
    pub provenance: ProjectionProvenance,
}

#[derive(Clone, Debug)]
pub struct ExpandedSettlementViewSnapshot {
    pub settlement_case_id: String,
    pub promise_intent_id: String,
    pub realm_id: String,
    pub current_settlement_status: String,
    pub total_funded_minor_units: i128,
    pub currency_code: String,
    pub latest_journal_entry_id: Option<String>,
    pub proof_status: String,
    pub proof_signal_count: i64,
    pub provenance: ProjectionProvenance,
}

#[derive(Clone, Debug)]
pub struct TrustSnapshot {
    pub account_id: String,
    pub realm_id: Option<String>,
    pub trust_posture: String,
    pub reason_codes: Vec<String>,
    pub promise_participation_count_90d: i64,
    pub funded_settlement_count_90d: i64,
    pub manual_review_case_bucket: String,
    pub proof_status: String,
    pub proof_signal_count: i64,
    pub provenance: ProjectionProvenance,
}

#[derive(Clone, Debug)]
pub struct ProjectionRebuildItem {
    pub projection_name: String,
    pub projection_row_count: i64,
    pub source_fact_count: i64,
    pub source_watermark_at: DateTime<Utc>,
    pub projection_lag_ms: i64,
}

#[derive(Clone, Debug)]
pub struct ProjectionRebuildOutcome {
    pub rebuild_generation: String,
    pub rebuilt_at: DateTime<Utc>,
    pub rebuilt: Vec<ProjectionRebuildItem>,
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
    pub(super) duplicate_callback: bool,
    pub(super) provider_submission_id: String,
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
