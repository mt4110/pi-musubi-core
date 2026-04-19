use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    SharedState,
    handlers::{
        ApiError, ApiResult, bad_request, internal_server_error, map_happy_route_error, not_found,
        require_bearer_token, require_internal_bearer_token, service_unavailable, unauthorized,
    },
    services::{
        happy_route::authorize_account,
        operator_review::{
            AppealCaseSnapshot, AttachEvidenceBundleInput, CreateAppealCaseInput,
            CreateReviewCaseInput, EvidenceAccessGrantSnapshot, EvidenceBundleSnapshot,
            GrantEvidenceAccessInput, OperatorDecisionFactSnapshot, OperatorReviewError,
            ReadReviewCaseSnapshot, RecordOperatorDecisionInput, ReviewCaseSnapshot,
            ReviewStatusReadModelSnapshot,
        },
    },
};

#[derive(Debug, Deserialize)]
pub struct CreateReviewCaseRequest {
    pub case_type: String,
    pub severity: String,
    pub subject_account_id: Option<String>,
    pub related_promise_intent_id: Option<String>,
    pub related_settlement_case_id: Option<String>,
    pub related_realm_id: Option<String>,
    pub opened_reason_code: String,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Option<Value>,
    pub assigned_operator_id: Option<String>,
    pub request_idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AttachEvidenceBundleRequest {
    pub evidence_visibility: String,
    pub summary_json: Option<Value>,
    pub raw_locator_json: Option<Value>,
    pub retention_class: String,
}

#[derive(Debug, Deserialize)]
pub struct GrantEvidenceAccessRequest {
    pub evidence_bundle_id: Option<String>,
    pub grantee_operator_id: String,
    pub access_scope: String,
    pub grant_reason: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RecordOperatorDecisionRequest {
    pub decision_kind: String,
    pub user_facing_reason_code: String,
    pub operator_note_internal: Option<String>,
    pub decision_payload_json: Option<Value>,
    pub decision_idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAppealCaseRequest {
    pub source_decision_fact_id: Option<String>,
    pub submitted_reason_code: String,
    pub appellant_statement: Option<String>,
    pub new_evidence_summary_json: Option<Value>,
    pub appeal_idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReviewCaseResponse {
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

#[derive(Debug, Serialize)]
pub struct EvidenceBundleResponse {
    pub evidence_bundle_id: String,
    pub review_case_id: String,
    pub evidence_visibility: String,
    pub summary_json: Value,
    pub retention_class: String,
    pub created_by_operator_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct EvidenceAccessGrantResponse {
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

#[derive(Debug, Serialize)]
pub struct OperatorDecisionFactResponse {
    pub operator_decision_fact_id: String,
    pub review_case_id: String,
    pub appeal_case_id: Option<String>,
    pub decision_kind: String,
    pub user_facing_reason_code: String,
    pub decided_by_operator_id: String,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AppealCaseResponse {
    pub appeal_case_id: String,
    pub source_review_case_id: String,
    pub source_decision_fact_id: Option<String>,
    pub appeal_status: String,
    pub submitted_by_account_id: String,
    pub submitted_reason_code: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ReadReviewCaseResponse {
    pub review_case: ReviewCaseResponse,
    pub evidence_bundles: Vec<EvidenceBundleResponse>,
    pub evidence_access_grants: Vec<EvidenceAccessGrantResponse>,
    pub operator_decision_facts: Vec<OperatorDecisionFactResponse>,
    pub appeal_cases: Vec<AppealCaseResponse>,
}

#[derive(Debug, Serialize)]
pub struct ReviewStatusReadModelResponse {
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

pub async fn create_review_case(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<CreateReviewCaseRequest>,
) -> ApiResult<ReviewCaseResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .operator_review
        .create_review_case(
            &operator_id,
            CreateReviewCaseInput {
                case_type: payload.case_type,
                severity: payload.severity,
                subject_account_id: payload.subject_account_id,
                related_promise_intent_id: payload.related_promise_intent_id,
                related_settlement_case_id: payload.related_settlement_case_id,
                related_realm_id: payload.related_realm_id,
                opened_reason_code: payload.opened_reason_code,
                source_fact_kind: payload.source_fact_kind,
                source_fact_id: payload.source_fact_id,
                source_snapshot_json: payload.source_snapshot_json.unwrap_or_else(|| {
                    serde_json::json!({
                        "source": "operator_review",
                        "note": "no source snapshot supplied"
                    })
                }),
                assigned_operator_id: payload.assigned_operator_id,
                request_idempotency_key: payload.request_idempotency_key,
            },
        )
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(review_case_response(snapshot)))
}

pub async fn list_review_cases(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<Vec<ReviewCaseResponse>> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshots = state
        .operator_review
        .list_review_cases(&operator_id)
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(
        snapshots.into_iter().map(review_case_response).collect(),
    ))
}

pub async fn read_review_case(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(review_case_id): Path<String>,
) -> ApiResult<ReadReviewCaseResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .operator_review
        .read_review_case(&operator_id, review_case_id.trim())
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(read_review_case_response(snapshot)))
}

pub async fn attach_evidence_bundle(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(review_case_id): Path<String>,
    Json(payload): Json<AttachEvidenceBundleRequest>,
) -> ApiResult<EvidenceBundleResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .operator_review
        .attach_evidence_bundle(
            &operator_id,
            review_case_id.trim(),
            AttachEvidenceBundleInput {
                evidence_visibility: payload.evidence_visibility,
                summary_json: payload
                    .summary_json
                    .unwrap_or_else(|| serde_json::json!({})),
                raw_locator_json: payload
                    .raw_locator_json
                    .unwrap_or_else(|| serde_json::json!({})),
                retention_class: payload.retention_class,
            },
        )
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(evidence_bundle_response(snapshot)))
}

pub async fn grant_evidence_access(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(review_case_id): Path<String>,
    Json(payload): Json<GrantEvidenceAccessRequest>,
) -> ApiResult<EvidenceAccessGrantResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .operator_review
        .grant_evidence_access(
            &operator_id,
            review_case_id.trim(),
            GrantEvidenceAccessInput {
                evidence_bundle_id: payload.evidence_bundle_id,
                grantee_operator_id: payload.grantee_operator_id,
                access_scope: payload.access_scope,
                grant_reason: payload.grant_reason,
                expires_at: payload.expires_at,
            },
        )
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(evidence_access_grant_response(snapshot)))
}

pub async fn record_operator_decision(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(review_case_id): Path<String>,
    Json(payload): Json<RecordOperatorDecisionRequest>,
) -> ApiResult<OperatorDecisionFactResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .operator_review
        .record_operator_decision(
            &operator_id,
            review_case_id.trim(),
            RecordOperatorDecisionInput {
                decision_kind: payload.decision_kind,
                user_facing_reason_code: payload.user_facing_reason_code,
                operator_note_internal: payload.operator_note_internal,
                decision_payload_json: payload
                    .decision_payload_json
                    .unwrap_or_else(|| serde_json::json!({})),
                decision_idempotency_key: payload.decision_idempotency_key,
            },
        )
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(operator_decision_fact_response(snapshot)))
}

pub async fn create_appeal_case(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(review_case_id): Path<String>,
    Json(payload): Json<CreateAppealCaseRequest>,
) -> ApiResult<AppealCaseResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = state
        .operator_review
        .create_appeal_case(
            &authenticated_account.account_id,
            review_case_id.trim(),
            CreateAppealCaseInput {
                source_decision_fact_id: payload.source_decision_fact_id,
                submitted_reason_code: payload.submitted_reason_code,
                appellant_statement: payload.appellant_statement,
                new_evidence_summary_json: payload
                    .new_evidence_summary_json
                    .unwrap_or_else(|| serde_json::json!({})),
                appeal_idempotency_key: payload.appeal_idempotency_key,
            },
        )
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(appeal_case_response(snapshot)))
}

pub async fn list_appeal_cases(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(review_case_id): Path<String>,
) -> ApiResult<Vec<AppealCaseResponse>> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshots = state
        .operator_review
        .list_appeal_cases_for_subject(&authenticated_account.account_id, review_case_id.trim())
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(
        snapshots.into_iter().map(appeal_case_response).collect(),
    ))
}

pub async fn get_review_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(review_case_id): Path<String>,
) -> ApiResult<ReviewStatusReadModelResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = state
        .operator_review
        .get_review_status_for_subject(&authenticated_account.account_id, review_case_id.trim())
        .await
        .map_err(map_operator_review_error)?;

    Ok(Json(review_status_read_model_response(snapshot)))
}

fn require_operator_id(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("x-musubi-operator-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| bad_request("x-musubi-operator-id header is required"))
}

fn map_operator_review_error(error: OperatorReviewError) -> ApiError {
    match error {
        OperatorReviewError::BadRequest(message) => bad_request(message),
        OperatorReviewError::Unauthorized(message) => unauthorized(message),
        OperatorReviewError::NotFound(message) => not_found(message),
        OperatorReviewError::Database {
            message, retryable, ..
        } => {
            eprintln!("database operator review error: {message}");
            if retryable {
                service_unavailable("temporarily unavailable")
            } else {
                internal_server_error("internal server error")
            }
        }
        OperatorReviewError::Internal(message) => {
            eprintln!("internal operator review error: {message}");
            internal_server_error("internal server error")
        }
    }
}

fn read_review_case_response(snapshot: ReadReviewCaseSnapshot) -> ReadReviewCaseResponse {
    ReadReviewCaseResponse {
        review_case: review_case_response(snapshot.review_case),
        evidence_bundles: snapshot
            .evidence_bundles
            .into_iter()
            .map(evidence_bundle_response)
            .collect(),
        evidence_access_grants: snapshot
            .evidence_access_grants
            .into_iter()
            .map(evidence_access_grant_response)
            .collect(),
        operator_decision_facts: snapshot
            .operator_decision_facts
            .into_iter()
            .map(operator_decision_fact_response)
            .collect(),
        appeal_cases: snapshot
            .appeal_cases
            .into_iter()
            .map(appeal_case_response)
            .collect(),
    }
}

fn review_case_response(snapshot: ReviewCaseSnapshot) -> ReviewCaseResponse {
    ReviewCaseResponse {
        review_case_id: snapshot.review_case_id,
        case_type: snapshot.case_type,
        severity: snapshot.severity,
        review_status: snapshot.review_status,
        subject_account_id: snapshot.subject_account_id,
        related_promise_intent_id: snapshot.related_promise_intent_id,
        related_settlement_case_id: snapshot.related_settlement_case_id,
        related_realm_id: snapshot.related_realm_id,
        opened_reason_code: snapshot.opened_reason_code,
        source_fact_kind: snapshot.source_fact_kind,
        source_fact_id: snapshot.source_fact_id,
        assigned_operator_id: snapshot.assigned_operator_id,
        opened_at: snapshot.opened_at,
        updated_at: snapshot.updated_at,
    }
}

fn evidence_bundle_response(snapshot: EvidenceBundleSnapshot) -> EvidenceBundleResponse {
    EvidenceBundleResponse {
        evidence_bundle_id: snapshot.evidence_bundle_id,
        review_case_id: snapshot.review_case_id,
        evidence_visibility: snapshot.evidence_visibility,
        summary_json: snapshot.summary_json,
        retention_class: snapshot.retention_class,
        created_by_operator_id: snapshot.created_by_operator_id,
        created_at: snapshot.created_at,
    }
}

fn evidence_access_grant_response(
    snapshot: EvidenceAccessGrantSnapshot,
) -> EvidenceAccessGrantResponse {
    EvidenceAccessGrantResponse {
        access_grant_id: snapshot.access_grant_id,
        review_case_id: snapshot.review_case_id,
        evidence_bundle_id: snapshot.evidence_bundle_id,
        grantee_operator_id: snapshot.grantee_operator_id,
        access_scope: snapshot.access_scope,
        grant_reason: snapshot.grant_reason,
        approved_by_operator_id: snapshot.approved_by_operator_id,
        expires_at: snapshot.expires_at,
        created_at: snapshot.created_at,
    }
}

fn operator_decision_fact_response(
    snapshot: OperatorDecisionFactSnapshot,
) -> OperatorDecisionFactResponse {
    OperatorDecisionFactResponse {
        operator_decision_fact_id: snapshot.operator_decision_fact_id,
        review_case_id: snapshot.review_case_id,
        appeal_case_id: snapshot.appeal_case_id,
        decision_kind: snapshot.decision_kind,
        user_facing_reason_code: snapshot.user_facing_reason_code,
        decided_by_operator_id: snapshot.decided_by_operator_id,
        recorded_at: snapshot.recorded_at,
    }
}

fn appeal_case_response(snapshot: AppealCaseSnapshot) -> AppealCaseResponse {
    AppealCaseResponse {
        appeal_case_id: snapshot.appeal_case_id,
        source_review_case_id: snapshot.source_review_case_id,
        source_decision_fact_id: snapshot.source_decision_fact_id,
        appeal_status: snapshot.appeal_status,
        submitted_by_account_id: snapshot.submitted_by_account_id,
        submitted_reason_code: snapshot.submitted_reason_code,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    }
}

fn review_status_read_model_response(
    snapshot: ReviewStatusReadModelSnapshot,
) -> ReviewStatusReadModelResponse {
    ReviewStatusReadModelResponse {
        review_case_id: snapshot.review_case_id,
        subject_account_id: snapshot.subject_account_id,
        related_promise_intent_id: snapshot.related_promise_intent_id,
        related_settlement_case_id: snapshot.related_settlement_case_id,
        related_realm_id: snapshot.related_realm_id,
        user_facing_status: snapshot.user_facing_status,
        user_facing_reason_code: snapshot.user_facing_reason_code,
        appeal_status: snapshot.appeal_status,
        evidence_requested: snapshot.evidence_requested,
        appeal_available: snapshot.appeal_available,
        latest_decision_fact_id: snapshot.latest_decision_fact_id,
        source_watermark_at: snapshot.source_watermark_at,
        source_fact_count: snapshot.source_fact_count,
        last_projected_at: snapshot.last_projected_at,
    }
}
