use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug)]
pub enum RoomProgressionError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Database {
        message: String,
        code: Option<String>,
        constraint: Option<String>,
        retryable: bool,
    },
    Internal(String),
}

impl RoomProgressionError {
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::Unauthorized(message)
            | Self::NotFound(message)
            | Self::Database { message, .. }
            | Self::Internal(message) => message,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CreateRoomProgressionInput {
    pub realm_id: String,
    pub participant_account_ids: Vec<String>,
    pub related_promise_intent_id: Option<String>,
    pub related_settlement_case_id: Option<String>,
    pub user_facing_reason_code: String,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Value,
    pub request_idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AppendRoomProgressionFactInput {
    pub transition_kind: String,
    pub to_stage: String,
    pub user_facing_reason_code: String,
    pub triggered_by_kind: String,
    pub triggered_by_account_id: Option<String>,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Value,
    pub review_case_id: Option<String>,
    pub fact_idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RoomProgressionTrackSnapshot {
    pub room_progression_id: String,
    pub realm_id: String,
    pub participant_a_account_id: String,
    pub participant_b_account_id: String,
    pub related_promise_intent_id: Option<String>,
    pub related_settlement_case_id: Option<String>,
    pub current_stage: String,
    pub current_status_code: String,
    pub current_user_facing_reason_code: String,
    pub current_review_case_id: Option<String>,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RoomProgressionFactSnapshot {
    pub room_progression_fact_id: String,
    pub room_progression_id: String,
    pub from_stage: String,
    pub to_stage: String,
    pub transition_kind: String,
    pub status_code: String,
    pub user_facing_reason_code: String,
    pub triggered_by_kind: String,
    pub triggered_by_account_id: Option<String>,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub review_case_id: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RoomProgressionViewSnapshot {
    pub room_progression_id: String,
    pub realm_id: String,
    pub participant_a_account_id: String,
    pub participant_b_account_id: String,
    pub visible_stage: String,
    pub status_code: String,
    pub user_facing_reason_code: String,
    pub review_case_id: Option<String>,
    pub review_pending: bool,
    pub review_status: Option<String>,
    pub appeal_available: bool,
    pub evidence_requested: bool,
    pub source_watermark_at: DateTime<Utc>,
    pub source_fact_count: i64,
    pub projection_lag_ms: i64,
    pub rebuild_generation: i64,
    pub last_projected_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RoomProgressionRebuildSnapshot {
    pub rebuilt_count: i64,
}
