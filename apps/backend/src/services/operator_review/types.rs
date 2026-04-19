use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug)]
pub enum OperatorReviewError {
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

impl OperatorReviewError {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorRole {
    Reviewer,
    Approver,
    Steward,
    Auditor,
    Support,
}

impl OperatorRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reviewer => "reviewer",
            Self::Approver => "approver",
            Self::Steward => "steward",
            Self::Auditor => "auditor",
            Self::Support => "support",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CreateReviewCaseInput {
    pub case_type: String,
    pub severity: String,
    pub subject_account_id: Option<String>,
    pub related_promise_intent_id: Option<String>,
    pub related_settlement_case_id: Option<String>,
    pub related_realm_id: Option<String>,
    pub opened_reason_code: String,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Value,
    pub assigned_operator_id: Option<String>,
    pub request_idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AttachEvidenceBundleInput {
    pub evidence_visibility: String,
    pub summary_json: Value,
    pub raw_locator_json: Value,
    pub retention_class: String,
}

#[derive(Clone, Debug)]
pub struct GrantEvidenceAccessInput {
    pub evidence_bundle_id: Option<String>,
    pub grantee_operator_id: String,
    pub access_scope: String,
    pub grant_reason: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RecordOperatorDecisionInput {
    pub decision_kind: String,
    pub user_facing_reason_code: String,
    pub operator_note_internal: Option<String>,
    pub decision_payload_json: Value,
    pub decision_idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CreateAppealCaseInput {
    pub source_decision_fact_id: Option<String>,
    pub submitted_reason_code: String,
    pub appellant_statement: Option<String>,
    pub new_evidence_summary_json: Value,
    pub appeal_idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ReviewCaseSnapshot {
    pub review_case_id: String,
    pub case_type: String,
    pub severity: String,
    pub review_status: String,
    pub subject_account_id: Option<String>,
    pub related_promise_intent_id: Option<String>,
    pub related_settlement_case_id: Option<String>,
    pub related_realm_id: Option<String>,
    pub opened_reason_code: String,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub assigned_operator_id: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct EvidenceBundleSnapshot {
    pub evidence_bundle_id: String,
    pub review_case_id: String,
    pub evidence_visibility: String,
    pub summary_json: Value,
    pub retention_class: String,
    pub created_by_operator_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct EvidenceAccessGrantSnapshot {
    pub access_grant_id: String,
    pub review_case_id: String,
    pub evidence_bundle_id: Option<String>,
    pub grantee_operator_id: String,
    pub access_scope: String,
    pub grant_reason: String,
    pub approved_by_operator_id: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct OperatorDecisionFactSnapshot {
    pub operator_decision_fact_id: String,
    pub review_case_id: String,
    pub appeal_case_id: Option<String>,
    pub decision_kind: String,
    pub user_facing_reason_code: String,
    pub decided_by_operator_id: String,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct AppealCaseSnapshot {
    pub appeal_case_id: String,
    pub source_review_case_id: String,
    pub source_decision_fact_id: Option<String>,
    pub appeal_status: String,
    pub submitted_by_account_id: String,
    pub submitted_reason_code: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ReadReviewCaseSnapshot {
    pub review_case: ReviewCaseSnapshot,
    pub evidence_bundles: Vec<EvidenceBundleSnapshot>,
    pub evidence_access_grants: Vec<EvidenceAccessGrantSnapshot>,
    pub operator_decision_facts: Vec<OperatorDecisionFactSnapshot>,
    pub appeal_cases: Vec<AppealCaseSnapshot>,
}

#[derive(Clone, Debug)]
pub struct ReviewStatusReadModelSnapshot {
    pub review_case_id: String,
    pub subject_account_id: Option<String>,
    pub related_promise_intent_id: Option<String>,
    pub related_settlement_case_id: Option<String>,
    pub related_realm_id: Option<String>,
    pub user_facing_status: String,
    pub user_facing_reason_code: String,
    pub appeal_status: String,
    pub evidence_requested: bool,
    pub appeal_available: bool,
    pub latest_decision_fact_id: Option<String>,
    pub source_watermark_at: DateTime<Utc>,
    pub source_fact_count: i64,
    pub last_projected_at: DateTime<Utc>,
}
