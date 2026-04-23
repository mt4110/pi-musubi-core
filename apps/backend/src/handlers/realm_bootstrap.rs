use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    SharedState,
    handlers::{
        ApiError, ApiResult, bad_request, internal_server_error, launch_blocked,
        map_happy_route_error, not_found, require_bearer_token, require_internal_bearer_token,
        require_operator_id, service_unavailable, unauthorized,
    },
    services::{
        happy_route::authorize_account,
        launch_posture::LaunchAction,
        realm_bootstrap::{
            CreateRealmAdmissionInput, CreateRealmRequestInput, CreateRealmSponsorRecordInput,
            ListRealmRequestsInput, RealmAdmissionSnapshot, RealmAdmissionViewSnapshot,
            RealmBootstrapError, RealmBootstrapRebuildSnapshot, RealmBootstrapSummarySnapshot,
            RealmBootstrapViewSnapshot, RealmRequestSnapshot, RealmReviewSummarySnapshot,
            RealmReviewTriggerSnapshot, RealmSnapshot, RealmSponsorRecordSnapshot,
            RejectRealmRequestInput, ReviewRealmRequestInput,
        },
    },
};

#[derive(Debug, Deserialize)]
pub struct CreateRealmRequestRequest {
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

#[derive(Debug, Deserialize)]
pub struct ReviewRealmRequestRequest {
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
    pub review_threshold_json: Option<Value>,
    pub review_decision_idempotency_key: String,
}

#[derive(Debug, Deserialize)]
pub struct RejectRealmRequestRequest {
    pub review_reason_code: String,
    pub review_decision_idempotency_key: String,
}

#[derive(Debug, Deserialize)]
pub struct ListRealmRequestsQuery {
    pub limit: Option<i64>,
    pub before_created_at: Option<DateTime<Utc>>,
    pub before_realm_request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRealmSponsorRecordRequest {
    pub sponsor_account_id: String,
    pub sponsor_status: String,
    pub quota_total: i64,
    pub status_reason_code: String,
    pub request_idempotency_key: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRealmAdmissionRequest {
    pub account_id: String,
    pub sponsor_record_id: Option<String>,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Option<Value>,
    pub request_idempotency_key: String,
}

#[derive(Debug, Serialize)]
pub struct RealmRequestResponse {
    pub realm_request_id: String,
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
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_realm_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct RealmRequestOperatorResponse {
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
    pub open_review_triggers: Vec<RealmReviewTriggerResponse>,
}

#[derive(Debug, Serialize)]
pub struct RealmResponse {
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

#[derive(Debug, Serialize)]
pub struct RealmSponsorRecordResponse {
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

#[derive(Debug, Serialize)]
pub struct RealmAdmissionResponse {
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

#[derive(Debug, Serialize)]
pub struct RealmBootstrapViewResponse {
    pub realm_id: String,
    pub slug: String,
    pub display_name: String,
    pub realm_status: String,
    pub admission_posture: String,
    pub corridor_status: String,
    pub public_reason_code: String,
    pub sponsor_display_state: String,
}

#[derive(Debug, Serialize)]
pub struct RealmAdmissionViewResponse {
    pub realm_id: String,
    pub account_id: String,
    pub admission_status: String,
    pub admission_kind: String,
    pub public_reason_code: String,
}

#[derive(Debug, Serialize)]
pub struct RealmBootstrapSummaryResponse {
    pub realm_request: Option<RealmRequestResponse>,
    pub bootstrap_view: RealmBootstrapViewResponse,
    pub admission_view: Option<RealmAdmissionViewResponse>,
}

#[derive(Debug, Serialize)]
pub struct RealmReviewTriggerResponse {
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

#[derive(Debug, Serialize)]
pub struct RealmReviewSummaryResponse {
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
    pub open_review_triggers: Vec<RealmReviewTriggerResponse>,
}

#[derive(Debug, Serialize)]
pub struct RealmBootstrapRebuildResponse {
    pub bootstrap_view_count: i64,
    pub admission_view_count: i64,
    pub review_summary_count: i64,
}

pub async fn create_realm_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<CreateRealmRequestRequest>,
) -> ApiResult<RealmRequestResponse> {
    let token = require_bearer_token(&headers)?;
    let account = authorize_account(&state, &token)
        .await
        .map_err(map_happy_route_error)?;
    state
        .launch_posture
        .check_participant_action(
            LaunchAction::RealmRequest,
            &account.account_id,
            Some(&account.pi_uid),
        )
        .await
        .map_err(|block| launch_blocked(block.status_code, block.message_code))?;
    let snapshot = state
        .realm_bootstrap
        .create_realm_request(
            &account.account_id,
            CreateRealmRequestInput {
                display_name: payload.display_name,
                slug_candidate: payload.slug_candidate,
                purpose_text: payload.purpose_text,
                venue_context_json: payload.venue_context_json,
                expected_member_shape_json: payload.expected_member_shape_json,
                bootstrap_rationale_text: payload.bootstrap_rationale_text,
                proposed_sponsor_account_id: payload.proposed_sponsor_account_id,
                proposed_steward_account_id: payload.proposed_steward_account_id,
                request_idempotency_key: payload.request_idempotency_key,
            },
        )
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_request_response(snapshot)))
}

pub async fn get_realm_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_request_id): Path<String>,
) -> ApiResult<RealmRequestResponse> {
    let token = require_bearer_token(&headers)?;
    let account = authorize_account(&state, &token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = state
        .realm_bootstrap
        .get_realm_request_for_requester(&account.account_id, realm_request_id.trim())
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_request_response(snapshot)))
}

pub async fn list_realm_requests(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListRealmRequestsQuery>,
) -> ApiResult<Vec<RealmRequestOperatorResponse>> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshots = state
        .realm_bootstrap
        .list_realm_requests_for_operator(
            &operator_id,
            ListRealmRequestsInput {
                limit: query.limit,
                before_created_at: query.before_created_at,
                before_realm_request_id: query.before_realm_request_id,
            },
        )
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(
        snapshots
            .into_iter()
            .map(realm_request_operator_response)
            .collect(),
    ))
}

pub async fn read_realm_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_request_id): Path<String>,
) -> ApiResult<RealmRequestOperatorResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .realm_bootstrap
        .read_realm_request_for_operator(&operator_id, realm_request_id.trim())
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_request_operator_response(snapshot)))
}

pub async fn approve_realm_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_request_id): Path<String>,
    Json(payload): Json<ReviewRealmRequestRequest>,
) -> ApiResult<RealmResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .realm_bootstrap
        .approve_realm_request(
            &operator_id,
            realm_request_id.trim(),
            ReviewRealmRequestInput {
                target_realm_status: payload.target_realm_status,
                approved_slug: payload.approved_slug,
                approved_display_name: payload.approved_display_name,
                review_reason_code: payload.review_reason_code,
                steward_account_id: payload.steward_account_id,
                sponsor_quota_total: payload.sponsor_quota_total,
                corridor_starts_at: payload.corridor_starts_at,
                corridor_ends_at: payload.corridor_ends_at,
                corridor_member_cap: payload.corridor_member_cap,
                corridor_sponsor_cap: payload.corridor_sponsor_cap,
                review_threshold_json: payload
                    .review_threshold_json
                    .unwrap_or_else(|| Value::Object(Default::default())),
                review_decision_idempotency_key: payload.review_decision_idempotency_key,
            },
        )
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_response(snapshot)))
}

pub async fn reject_realm_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_request_id): Path<String>,
    Json(payload): Json<RejectRealmRequestRequest>,
) -> ApiResult<RealmRequestOperatorResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .realm_bootstrap
        .reject_realm_request(
            &operator_id,
            realm_request_id.trim(),
            RejectRealmRequestInput {
                review_reason_code: payload.review_reason_code,
                review_decision_idempotency_key: payload.review_decision_idempotency_key,
            },
        )
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_request_operator_response(snapshot)))
}

pub async fn create_realm_sponsor_record(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_id): Path<String>,
    Json(payload): Json<CreateRealmSponsorRecordRequest>,
) -> ApiResult<RealmSponsorRecordResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .realm_bootstrap
        .create_realm_sponsor_record(
            &operator_id,
            realm_id.trim(),
            CreateRealmSponsorRecordInput {
                sponsor_account_id: payload.sponsor_account_id,
                sponsor_status: payload.sponsor_status,
                quota_total: payload.quota_total,
                status_reason_code: payload.status_reason_code,
                request_idempotency_key: payload.request_idempotency_key,
            },
        )
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_sponsor_record_response(snapshot)))
}

pub async fn create_realm_admission(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_id): Path<String>,
    Json(payload): Json<CreateRealmAdmissionRequest>,
) -> ApiResult<RealmAdmissionResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    state
        .realm_bootstrap
        .ensure_operator_write_role(&operator_id)
        .await
        .map_err(map_realm_bootstrap_error)?;
    state
        .launch_posture
        .check_participant_action(
            LaunchAction::RealmAdmission,
            payload.account_id.trim(),
            None,
        )
        .await
        .map_err(|block| launch_blocked(block.status_code, block.message_code))?;
    let snapshot = state
        .realm_bootstrap
        .create_realm_admission(
            &operator_id,
            realm_id.trim(),
            CreateRealmAdmissionInput {
                account_id: payload.account_id,
                sponsor_record_id: payload.sponsor_record_id,
                source_fact_kind: payload.source_fact_kind,
                source_fact_id: payload.source_fact_id,
                source_snapshot_json: payload
                    .source_snapshot_json
                    .unwrap_or_else(|| Value::Object(Default::default())),
                request_idempotency_key: payload.request_idempotency_key,
            },
        )
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_admission_response(snapshot)))
}

pub async fn get_bootstrap_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_id): Path<String>,
) -> ApiResult<RealmBootstrapSummaryResponse> {
    let token = require_bearer_token(&headers)?;
    let account = authorize_account(&state, &token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = state
        .realm_bootstrap
        .get_bootstrap_summary_for_viewer(&account.account_id, realm_id.trim())
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_bootstrap_summary_response(snapshot)))
}

pub async fn get_review_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(realm_id): Path<String>,
) -> ApiResult<RealmReviewSummaryResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .realm_bootstrap
        .get_review_summary_for_operator(&operator_id, realm_id.trim())
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_review_summary_response(snapshot)))
}

pub async fn rebuild_realm_bootstrap_views(
    State(state): State<SharedState>,
    headers: HeaderMap,
    _body: Bytes,
) -> ApiResult<RealmBootstrapRebuildResponse> {
    require_internal_bearer_token(&headers)?;
    let operator_id = require_operator_id(&headers)?;
    let snapshot = state
        .realm_bootstrap
        .rebuild_realm_bootstrap_views(&operator_id)
        .await
        .map_err(map_realm_bootstrap_error)?;
    Ok(Json(realm_bootstrap_rebuild_response(snapshot)))
}

fn realm_request_response(snapshot: RealmRequestSnapshot) -> RealmRequestResponse {
    RealmRequestResponse {
        realm_request_id: snapshot.realm_request_id,
        display_name: snapshot.display_name,
        slug_candidate: snapshot.slug_candidate,
        purpose_text: snapshot.purpose_text,
        venue_context_json: snapshot.venue_context_json,
        expected_member_shape_json: snapshot.expected_member_shape_json,
        bootstrap_rationale_text: snapshot.bootstrap_rationale_text,
        proposed_sponsor_account_id: snapshot.proposed_sponsor_account_id,
        proposed_steward_account_id: snapshot.proposed_steward_account_id,
        request_state: snapshot.request_state,
        review_reason_code: snapshot.review_reason_code,
        reviewed_at: snapshot.reviewed_at,
        created_realm_id: snapshot.created_realm_id,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    }
}

fn realm_request_operator_response(snapshot: RealmRequestSnapshot) -> RealmRequestOperatorResponse {
    RealmRequestOperatorResponse {
        realm_request_id: snapshot.realm_request_id,
        requested_by_account_id: snapshot.requested_by_account_id,
        display_name: snapshot.display_name,
        slug_candidate: snapshot.slug_candidate,
        purpose_text: snapshot.purpose_text,
        venue_context_json: snapshot.venue_context_json,
        expected_member_shape_json: snapshot.expected_member_shape_json,
        bootstrap_rationale_text: snapshot.bootstrap_rationale_text,
        proposed_sponsor_account_id: snapshot.proposed_sponsor_account_id,
        proposed_steward_account_id: snapshot.proposed_steward_account_id,
        request_state: snapshot.request_state,
        review_reason_code: snapshot.review_reason_code,
        reviewed_by_operator_id: snapshot.reviewed_by_operator_id,
        reviewed_at: snapshot.reviewed_at,
        created_realm_id: snapshot.created_realm_id,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
        open_review_triggers: snapshot
            .open_review_triggers
            .into_iter()
            .map(realm_review_trigger_response)
            .collect(),
    }
}

fn realm_response(snapshot: RealmSnapshot) -> RealmResponse {
    RealmResponse {
        realm_id: snapshot.realm_id,
        slug: snapshot.slug,
        display_name: snapshot.display_name,
        realm_status: snapshot.realm_status,
        public_reason_code: snapshot.public_reason_code,
        created_from_realm_request_id: snapshot.created_from_realm_request_id,
        steward_account_id: snapshot.steward_account_id,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    }
}

fn realm_sponsor_record_response(
    snapshot: RealmSponsorRecordSnapshot,
) -> RealmSponsorRecordResponse {
    RealmSponsorRecordResponse {
        realm_sponsor_record_id: snapshot.realm_sponsor_record_id,
        realm_id: snapshot.realm_id,
        sponsor_account_id: snapshot.sponsor_account_id,
        sponsor_status: snapshot.sponsor_status,
        quota_total: snapshot.quota_total,
        status_reason_code: snapshot.status_reason_code,
        approved_by_operator_id: snapshot.approved_by_operator_id,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    }
}

fn realm_admission_response(snapshot: RealmAdmissionSnapshot) -> RealmAdmissionResponse {
    RealmAdmissionResponse {
        realm_admission_id: snapshot.realm_admission_id,
        realm_id: snapshot.realm_id,
        account_id: snapshot.account_id,
        admission_kind: snapshot.admission_kind,
        admission_status: snapshot.admission_status,
        sponsor_record_id: snapshot.sponsor_record_id,
        bootstrap_corridor_id: snapshot.bootstrap_corridor_id,
        granted_by_actor_kind: snapshot.granted_by_actor_kind,
        granted_by_actor_id: snapshot.granted_by_actor_id,
        review_reason_code: snapshot.review_reason_code,
        source_fact_kind: snapshot.source_fact_kind,
        source_fact_id: snapshot.source_fact_id,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    }
}

fn realm_bootstrap_view_response(
    snapshot: RealmBootstrapViewSnapshot,
) -> RealmBootstrapViewResponse {
    RealmBootstrapViewResponse {
        realm_id: snapshot.realm_id,
        slug: snapshot.slug,
        display_name: snapshot.display_name,
        realm_status: snapshot.realm_status,
        admission_posture: snapshot.admission_posture,
        corridor_status: snapshot.corridor_status,
        public_reason_code: snapshot.public_reason_code,
        sponsor_display_state: snapshot.sponsor_display_state,
    }
}

fn realm_admission_view_response(
    snapshot: RealmAdmissionViewSnapshot,
) -> RealmAdmissionViewResponse {
    RealmAdmissionViewResponse {
        realm_id: snapshot.realm_id,
        account_id: snapshot.account_id,
        admission_status: snapshot.admission_status,
        admission_kind: snapshot.admission_kind,
        public_reason_code: snapshot.public_reason_code,
    }
}

fn realm_review_trigger_response(
    snapshot: RealmReviewTriggerSnapshot,
) -> RealmReviewTriggerResponse {
    RealmReviewTriggerResponse {
        realm_review_trigger_id: snapshot.realm_review_trigger_id,
        realm_id: snapshot.realm_id,
        trigger_kind: snapshot.trigger_kind,
        trigger_state: snapshot.trigger_state,
        redacted_reason_code: snapshot.redacted_reason_code,
        related_account_id: snapshot.related_account_id,
        related_realm_request_id: snapshot.related_realm_request_id,
        related_sponsor_record_id: snapshot.related_sponsor_record_id,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
        resolved_at: snapshot.resolved_at,
    }
}

fn realm_review_summary_response(
    snapshot: RealmReviewSummarySnapshot,
) -> RealmReviewSummaryResponse {
    RealmReviewSummaryResponse {
        realm_id: snapshot.realm_id,
        realm_status: snapshot.realm_status,
        corridor_status: snapshot.corridor_status,
        corridor_remaining_seconds: snapshot.corridor_remaining_seconds,
        active_sponsor_count: snapshot.active_sponsor_count,
        sponsor_backed_admission_count: snapshot.sponsor_backed_admission_count,
        recent_admission_count_7d: snapshot.recent_admission_count_7d,
        open_review_trigger_count: snapshot.open_review_trigger_count,
        open_review_case_count: snapshot.open_review_case_count,
        latest_redacted_reason_code: snapshot.latest_redacted_reason_code,
        source_watermark_at: snapshot.source_watermark_at,
        source_fact_count: snapshot.source_fact_count,
        projection_lag_ms: snapshot.projection_lag_ms,
        rebuild_generation: snapshot.rebuild_generation,
        last_projected_at: snapshot.last_projected_at,
        open_review_triggers: snapshot
            .open_review_triggers
            .into_iter()
            .map(realm_review_trigger_response)
            .collect(),
    }
}

fn realm_bootstrap_summary_response(
    snapshot: RealmBootstrapSummarySnapshot,
) -> RealmBootstrapSummaryResponse {
    RealmBootstrapSummaryResponse {
        realm_request: snapshot.realm_request.map(realm_request_response),
        bootstrap_view: realm_bootstrap_view_response(snapshot.bootstrap_view),
        admission_view: snapshot.admission_view.map(realm_admission_view_response),
    }
}

fn realm_bootstrap_rebuild_response(
    snapshot: RealmBootstrapRebuildSnapshot,
) -> RealmBootstrapRebuildResponse {
    RealmBootstrapRebuildResponse {
        bootstrap_view_count: snapshot.bootstrap_view_count,
        admission_view_count: snapshot.admission_view_count,
        review_summary_count: snapshot.review_summary_count,
    }
}

fn map_realm_bootstrap_error(error: RealmBootstrapError) -> ApiError {
    match error {
        RealmBootstrapError::BadRequest(message) => bad_request(message),
        RealmBootstrapError::Unauthorized(message) => unauthorized(message),
        RealmBootstrapError::NotFound(message) => not_found(message),
        RealmBootstrapError::Database {
            message,
            code,
            constraint,
            retryable,
        } => {
            eprintln!(
                "database realm bootstrap error: message={message}; code={}; constraint={}",
                code.as_deref().unwrap_or("none"),
                constraint.as_deref().unwrap_or("none")
            );
            if retryable {
                service_unavailable("temporarily unavailable")
            } else {
                internal_server_error("internal server error")
            }
        }
        RealmBootstrapError::Internal(message) => {
            eprintln!("internal realm bootstrap error: {message}");
            internal_server_error("internal server error")
        }
    }
}
