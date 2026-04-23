use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug)]
pub enum RealmBootstrapError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Database {
        message: String,
        code: Option<String>,
        constraint: Option<String>,
        retryable: bool,
    },
    LaunchBlocked {
        status_code: axum::http::StatusCode,
        message_code: &'static str,
    },
    Internal(String),
}

impl RealmBootstrapError {
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::Unauthorized(message)
            | Self::NotFound(message)
            | Self::Database { message, .. }
            | Self::Internal(message) => message,
            Self::LaunchBlocked { message_code, .. } => message_code,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CreateRealmRequestInput {
    pub display_name: String,
    pub slug_candidate: String,
    pub purpose_text: String,
    pub venue_context_json: Value,
    pub expected_member_shape_json: Value,
    pub bootstrap_rationale_text: String,
    pub proposed_sponsor_account_id: Option<String>,
    pub proposed_steward_account_id: Option<String>,
    pub request_idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct ReviewRealmRequestInput {
    pub target_realm_status: String,
    pub approved_slug: Option<String>,
    pub approved_display_name: Option<String>,
    pub review_reason_code: String,
    pub steward_account_id: Option<String>,
    pub sponsor_quota_total: Option<i64>,
    pub corridor_starts_at: Option<DateTime<Utc>>,
    pub corridor_ends_at: Option<DateTime<Utc>>,
    pub corridor_member_cap: Option<i64>,
    pub corridor_sponsor_cap: Option<i64>,
    pub review_threshold_json: Value,
    pub review_decision_idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct RejectRealmRequestInput {
    pub review_reason_code: String,
    pub review_decision_idempotency_key: String,
}

#[derive(Clone, Debug, Default)]
pub struct ListRealmRequestsInput {
    pub limit: Option<i64>,
    pub before_created_at: Option<DateTime<Utc>>,
    pub before_realm_request_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CreateRealmSponsorRecordInput {
    pub sponsor_account_id: String,
    pub sponsor_status: String,
    pub quota_total: i64,
    pub status_reason_code: String,
    pub request_idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct CreateRealmAdmissionInput {
    pub account_id: String,
    pub sponsor_record_id: Option<String>,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Value,
    pub request_idempotency_key: String,
}

#[derive(Clone, Debug)]
pub struct RealmRequestSnapshot {
    pub realm_request_id: String,
    pub requested_by_account_id: String,
    pub display_name: String,
    pub slug_candidate: String,
    pub purpose_text: String,
    pub venue_context_json: Value,
    pub expected_member_shape_json: Value,
    pub bootstrap_rationale_text: String,
    pub proposed_sponsor_account_id: Option<String>,
    pub proposed_steward_account_id: Option<String>,
    pub request_state: String,
    pub review_reason_code: String,
    pub reviewed_by_operator_id: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_realm_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub open_review_triggers: Vec<RealmReviewTriggerSnapshot>,
}

#[derive(Clone, Debug)]
pub struct RealmSnapshot {
    pub realm_id: String,
    pub slug: String,
    pub display_name: String,
    pub realm_status: String,
    pub public_reason_code: String,
    pub created_from_realm_request_id: String,
    pub steward_account_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RealmSponsorRecordSnapshot {
    pub realm_sponsor_record_id: String,
    pub realm_id: String,
    pub sponsor_account_id: String,
    pub sponsor_status: String,
    pub quota_total: i64,
    pub status_reason_code: String,
    pub approved_by_operator_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct BootstrapCorridorSnapshot {
    pub bootstrap_corridor_id: String,
    pub realm_id: String,
    pub corridor_status: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub member_cap: i64,
    pub sponsor_cap: i64,
    pub created_by_operator_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RealmAdmissionSnapshot {
    pub realm_admission_id: String,
    pub realm_id: String,
    pub account_id: String,
    pub admission_kind: String,
    pub admission_status: String,
    pub sponsor_record_id: Option<String>,
    pub bootstrap_corridor_id: Option<String>,
    pub granted_by_actor_kind: String,
    pub granted_by_actor_id: String,
    pub review_reason_code: String,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RealmReviewTriggerSnapshot {
    pub realm_review_trigger_id: String,
    pub realm_id: Option<String>,
    pub trigger_kind: String,
    pub trigger_state: String,
    pub redacted_reason_code: String,
    pub related_account_id: Option<String>,
    pub related_realm_request_id: Option<String>,
    pub related_sponsor_record_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct RealmBootstrapViewSnapshot {
    pub realm_id: String,
    pub slug: String,
    pub display_name: String,
    pub realm_status: String,
    pub admission_posture: String,
    pub corridor_status: String,
    pub public_reason_code: String,
    pub sponsor_display_state: String,
    pub source_watermark_at: DateTime<Utc>,
    pub source_fact_count: i64,
    pub projection_lag_ms: i64,
    pub rebuild_generation: i64,
    pub last_projected_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RealmAdmissionViewSnapshot {
    pub realm_id: String,
    pub account_id: String,
    pub admission_status: String,
    pub admission_kind: String,
    pub public_reason_code: String,
    pub source_watermark_at: DateTime<Utc>,
    pub source_fact_count: i64,
    pub projection_lag_ms: i64,
    pub rebuild_generation: i64,
    pub last_projected_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RealmReviewSummarySnapshot {
    pub realm_id: String,
    pub realm_status: String,
    pub corridor_status: String,
    pub corridor_remaining_seconds: i64,
    pub active_sponsor_count: i64,
    pub sponsor_backed_admission_count: i64,
    pub recent_admission_count_7d: i64,
    pub open_review_trigger_count: i64,
    pub open_review_case_count: i64,
    pub latest_redacted_reason_code: String,
    pub source_watermark_at: DateTime<Utc>,
    pub source_fact_count: i64,
    pub projection_lag_ms: i64,
    pub rebuild_generation: i64,
    pub last_projected_at: DateTime<Utc>,
    pub open_review_triggers: Vec<RealmReviewTriggerSnapshot>,
}

#[derive(Clone, Debug)]
pub struct RealmBootstrapSummarySnapshot {
    pub realm_request: Option<RealmRequestSnapshot>,
    pub bootstrap_view: RealmBootstrapViewSnapshot,
    pub admission_view: Option<RealmAdmissionViewSnapshot>,
}

#[derive(Clone, Debug)]
pub struct RealmBootstrapRebuildSnapshot {
    pub bootstrap_view_count: i64,
    pub admission_view_count: i64,
    pub review_summary_count: i64,
}
