use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug)]
pub enum OpsObservabilityError {
    Database { message: String, retryable: bool },
    Internal(String),
}

impl OpsObservabilityError {
    pub fn message(&self) -> &str {
        match self {
            Self::Database { message, .. } | Self::Internal(message) => message,
        }
    }
}

impl From<tokio_postgres::Error> for OpsObservabilityError {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::Database {
            message: error.to_string(),
            retryable: false,
        }
    }
}

impl From<musubi_db_runtime::DbRuntimeError> for OpsObservabilityError {
    fn from(error: musubi_db_runtime::DbRuntimeError) -> Self {
        Self::Database {
            message: error.to_string(),
            retryable: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OpsSliMetric {
    pub metric_name: String,
    pub status: String,
    pub value: Option<Value>,
    pub reason: Option<String>,
}

impl OpsSliMetric {
    pub fn ok(metric_name: impl Into<String>, value: Value) -> Self {
        Self {
            metric_name: metric_name.into(),
            status: "ok".to_owned(),
            value: Some(value),
            reason: None,
        }
    }

    pub fn unknown(metric_name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            metric_name: metric_name.into(),
            status: "unknown".to_owned(),
            value: None,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OpsHealthSnapshot {
    pub status: String,
    pub service: String,
    pub checked_at: DateTime<Utc>,
    pub database: OpsSliMetric,
}

#[derive(Clone, Debug, Serialize)]
pub struct OpsReadinessSnapshot {
    pub status: String,
    pub service: String,
    pub checked_at: DateTime<Utc>,
    pub database: OpsSliMetric,
    pub migrations: MigrationReadinessSnapshot,
}

#[derive(Clone, Debug, Serialize)]
pub struct MigrationReadinessSnapshot {
    pub status: String,
    pub required_latest_schema: bool,
    pub bootstrap_required: bool,
    pub migration_lock_available: bool,
    pub applied_count: usize,
    pub pending_count: usize,
    pub failed_count: usize,
    pub unexpected_applied_count: usize,
    pub checksum_drift_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct OpsObservabilitySnapshot {
    pub status: String,
    pub service: String,
    pub generated_at: DateTime<Utc>,
    pub database: OpsSliMetric,
    pub projection_lag: Vec<ProjectionLagSummary>,
    pub operator_review_queue: OperatorReviewQueueSummary,
    pub realm_review_triggers: RealmReviewTriggerSummary,
    pub orchestration_backlog: OrchestrationBacklogSummary,
    pub unsupported_metrics: Vec<OpsSliMetric>,
    pub boundary: ObservabilityBoundarySnapshot,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectionLagSummary {
    pub projection_name: String,
    pub status: String,
    pub row_count: Option<i64>,
    pub stale_row_count: Option<i64>,
    pub max_projection_lag_ms: Option<i64>,
    pub latest_source_watermark_at: Option<DateTime<Utc>>,
    pub latest_projected_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OperatorReviewQueueSummary {
    pub status: String,
    pub open_case_count: Option<i64>,
    pub awaiting_evidence_count: Option<i64>,
    pub appealed_case_count: Option<i64>,
    pub oldest_opened_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RealmReviewTriggerSummary {
    pub status: String,
    pub open_trigger_count: Option<i64>,
    pub oldest_open_trigger_at: Option<DateTime<Utc>>,
    pub latest_redacted_reason_code: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrchestrationBacklogSummary {
    pub status: String,
    pub outbox_pending_count: Option<i64>,
    pub outbox_processing_count: Option<i64>,
    pub outbox_quarantined_count: Option<i64>,
    pub inbox_pending_count: Option<i64>,
    pub inbox_processing_count: Option<i64>,
    pub inbox_quarantined_count: Option<i64>,
    pub oldest_available_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ObservabilityBoundarySnapshot {
    pub observability_is_business_truth: bool,
    pub projection_lag_is_writer_decision_input: bool,
    pub participant_visible: bool,
    pub raw_evidence_visible: bool,
    pub operator_notes_visible: bool,
    pub source_identifiers_visible: bool,
    pub pii_visible: bool,
}
