use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{OrchestrationError, RetryClass};

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoritativeChange {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub change_type: String,
    pub payload_json: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewOutboxMessage {
    pub event_id: Uuid,
    pub idempotency_key: Uuid,
    pub stream_key: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub schema_version: i32,
    pub payload_json: Value,
    pub payload_hash: String,
    pub available_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl NewOutboxMessage {
    pub fn new(
        event_id: Uuid,
        idempotency_key: Uuid,
        stream_key: impl Into<String>,
        aggregate_type: impl Into<String>,
        aggregate_id: Uuid,
        event_type: impl Into<String>,
        schema_version: i32,
        payload_json: Value,
        available_at: DateTime<Utc>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, OrchestrationError> {
        Ok(Self {
            event_id,
            idempotency_key,
            stream_key: stream_key.into(),
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            event_type: event_type.into(),
            schema_version,
            payload_hash: payload_hash(&payload_json)?,
            payload_json,
            available_at,
            created_at,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboxDeliveryStatus {
    Pending,
    Processing,
    Published,
    Failed,
    Quarantined,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutboxMessage {
    pub event_id: Uuid,
    pub idempotency_key: Uuid,
    pub stream_key: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub schema_version: i32,
    pub payload_json: Value,
    pub payload_hash: String,
    pub delivery_status: OutboxDeliveryStatus,
    pub attempt_count: u32,
    pub available_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error_class: Option<RetryClass>,
    pub last_error_code: Option<String>,
    pub last_error_detail: Option<String>,
    pub claimed_by: Option<String>,
    pub claimed_until: Option<DateTime<Utc>>,
    pub quarantined_at: Option<DateTime<Utc>>,
    pub quarantine_reason: Option<QuarantineReason>,
    pub retain_until: Option<DateTime<Utc>>,
    pub causal_order: i64,
    pub published_external_idempotency_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClaimedOutboxMessage {
    pub message: OutboxMessage,
    pub relay_name: String,
    pub claimed_at: DateTime<Utc>,
    pub claimed_until: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutboxAttempt {
    pub event_id: Uuid,
    pub attempt_number: u32,
    pub relay_name: String,
    pub claimed_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub failure_class: Option<RetryClass>,
    pub failure_code: Option<String>,
    pub failure_detail: Option<String>,
    pub external_idempotency_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExternalIdempotencyKey(String);

impl ExternalIdempotencyKey {
    pub fn new(value: impl Into<String>) -> Result<Self, OrchestrationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(OrchestrationError::EmptyExternalIdempotencyKey);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryReceipt {
    pub external_idempotency_key: ExternalIdempotencyKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuarantineReason {
    PoisonPill,
    PermanentFailure,
    AttemptBudgetExceeded,
    CompatibilityWindowExpired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessingFailure {
    pub class: RetryClass,
    pub code: String,
    pub detail: String,
    pub quarantine_reason: QuarantineReason,
}

impl ProcessingFailure {
    pub fn transient(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            class: RetryClass::Transient,
            code: code.into(),
            detail: detail.into(),
            quarantine_reason: QuarantineReason::AttemptBudgetExceeded,
        }
    }

    pub fn permanent(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            class: RetryClass::Permanent,
            code: code.into(),
            detail: detail.into(),
            quarantine_reason: QuarantineReason::PermanentFailure,
        }
    }

    pub fn deferred(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            class: RetryClass::Deferred,
            code: code.into(),
            detail: detail.into(),
            quarantine_reason: QuarantineReason::CompatibilityWindowExpired,
        }
    }

    pub fn poison_pill(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            class: RetryClass::Permanent,
            code: code.into(),
            detail: detail.into(),
            quarantine_reason: QuarantineReason::PoisonPill,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Idle,
    Published {
        event_id: Uuid,
    },
    RetryScheduled {
        event_id: Uuid,
        retry_at: DateTime<Utc>,
    },
    Quarantined {
        event_id: Uuid,
        reason: QuarantineReason,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandEnvelope {
    pub command_id: Uuid,
    pub source_event_id: Uuid,
    pub command_type: String,
    pub schema_version: i32,
    pub payload_json: Value,
    pub payload_hash: String,
}

impl CommandEnvelope {
    pub fn new(
        command_id: Uuid,
        source_event_id: Uuid,
        command_type: impl Into<String>,
        schema_version: i32,
        payload_json: Value,
    ) -> Result<Self, OrchestrationError> {
        Ok(Self {
            command_id,
            source_event_id,
            command_type: command_type.into(),
            schema_version,
            payload_hash: payload_hash(&payload_json)?,
            payload_json,
        })
    }

    pub fn matches_inbox_entry(&self, entry: &CommandInboxEntry) -> bool {
        self.source_event_id == entry.source_event_id
            && self.command_type == entry.command_type
            && self.schema_version == entry.schema_version
            && self.payload_hash == entry.payload_hash
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandInboxStatus {
    Pending,
    Processing,
    Completed,
    Quarantined,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CommandKey {
    pub consumer_name: String,
    pub command_id: Uuid,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandInboxEntry {
    pub key: CommandKey,
    pub source_event_id: Uuid,
    pub command_type: String,
    pub schema_version: i32,
    pub payload_hash: String,
    pub status: CommandInboxStatus,
    pub attempt_count: u32,
    pub first_seen_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub claimed_by: Option<String>,
    pub claimed_until: Option<DateTime<Utc>>,
    pub last_error_class: Option<RetryClass>,
    pub last_error_code: Option<String>,
    pub last_error_detail: Option<String>,
    pub quarantined_at: Option<DateTime<Utc>>,
    pub quarantine_reason: Option<QuarantineReason>,
    pub result_type: Option<String>,
    pub result_json: Option<Value>,
    pub retain_until: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandBeginOutcome {
    FirstSeen(CommandInboxEntry),
    ReadyForRetry(CommandInboxEntry),
    Duplicate(CommandInboxEntry),
    Deferred(CommandInboxEntry),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandCompletion {
    pub result_type: String,
    pub result_json: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsumeOutcome {
    Completed {
        command_id: Uuid,
    },
    RetryScheduled {
        command_id: Uuid,
        retry_at: DateTime<Utc>,
    },
    Duplicate {
        command_id: Uuid,
    },
    Deferred {
        command_id: Uuid,
        retry_at: DateTime<Utc>,
    },
    Quarantined {
        command_id: Uuid,
        reason: QuarantineReason,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedOutboxMessage {
    pub message: OutboxMessage,
    pub attempts: Vec<OutboxAttempt>,
    pub pruned_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedCommandInboxEntry {
    pub entry: CommandInboxEntry,
    pub pruned_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PruneOutcome {
    pub pruned_outbox_event_ids: Vec<Uuid>,
    pub pruned_command_keys: Vec<CommandKey>,
}

fn payload_hash(payload_json: &Value) -> Result<String, OrchestrationError> {
    let payload_bytes = serde_json::to_vec(payload_json)
        .map_err(|error| OrchestrationError::PayloadHashEncodingFailed(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(payload_bytes)))
}
