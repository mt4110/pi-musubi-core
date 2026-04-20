use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    SharedState,
    handlers::{
        ApiError, ApiResult, bad_request, internal_server_error, map_happy_route_error, not_found,
        require_bearer_token, require_internal_bearer_token, require_operator_id,
        service_unavailable, unauthorized,
    },
    services::{
        happy_route::authorize_account,
        room_progression::{
            AppendRoomProgressionFactInput, CreateRoomProgressionInput, RoomProgressionError,
            RoomProgressionFactSnapshot, RoomProgressionRebuildSnapshot,
            RoomProgressionTrackSnapshot, RoomProgressionViewSnapshot,
        },
    },
};

#[derive(Debug, Deserialize)]
pub struct CreateRoomProgressionRequest {
    pub realm_id: String,
    pub participant_account_ids: Vec<String>,
    pub related_promise_intent_id: Option<String>,
    pub related_settlement_case_id: Option<String>,
    pub user_facing_reason_code: String,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Option<Value>,
    pub request_idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppendRoomProgressionFactRequest {
    pub transition_kind: String,
    pub to_stage: String,
    pub user_facing_reason_code: String,
    pub triggered_by_kind: String,
    pub triggered_by_account_id: Option<String>,
    pub source_fact_kind: String,
    pub source_fact_id: String,
    pub source_snapshot_json: Option<Value>,
    pub review_case_id: Option<String>,
    pub fact_idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoomProgressionTrackResponse {
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

#[derive(Debug, Serialize)]
pub struct RoomProgressionFactResponse {
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

#[derive(Debug, Serialize)]
pub struct RoomProgressionViewResponse {
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

#[derive(Debug, Serialize)]
pub struct RoomProgressionRebuildResponse {
    pub rebuilt_count: i64,
}

pub async fn create_room_progression(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<CreateRoomProgressionRequest>,
) -> ApiResult<RoomProgressionTrackResponse> {
    require_internal_bearer_token(&headers)?;
    let snapshot = state
        .room_progression
        .create_room_progression(CreateRoomProgressionInput {
            realm_id: payload.realm_id,
            participant_account_ids: payload.participant_account_ids,
            related_promise_intent_id: payload.related_promise_intent_id,
            related_settlement_case_id: payload.related_settlement_case_id,
            user_facing_reason_code: payload.user_facing_reason_code,
            source_fact_kind: payload.source_fact_kind,
            source_fact_id: payload.source_fact_id,
            source_snapshot_json: payload
                .source_snapshot_json
                .unwrap_or_else(|| serde_json::json!({})),
            request_idempotency_key: payload.request_idempotency_key,
        })
        .await
        .map_err(map_room_progression_error)?;

    Ok(Json(room_progression_track_response(snapshot)))
}

pub async fn append_room_progression_fact(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(room_progression_id): Path<String>,
    Json(payload): Json<AppendRoomProgressionFactRequest>,
) -> ApiResult<RoomProgressionFactResponse> {
    require_internal_bearer_token(&headers)?;
    let triggered_by_account_id = resolved_triggered_by_account_id(
        &headers,
        &payload.triggered_by_kind,
        &payload.triggered_by_account_id,
    )?;
    let snapshot = state
        .room_progression
        .append_room_progression_fact(
            room_progression_id.trim(),
            AppendRoomProgressionFactInput {
                transition_kind: payload.transition_kind,
                to_stage: payload.to_stage,
                user_facing_reason_code: payload.user_facing_reason_code,
                triggered_by_kind: payload.triggered_by_kind,
                triggered_by_account_id,
                source_fact_kind: payload.source_fact_kind,
                source_fact_id: payload.source_fact_id,
                source_snapshot_json: payload
                    .source_snapshot_json
                    .unwrap_or_else(|| serde_json::json!({})),
                review_case_id: payload.review_case_id,
                fact_idempotency_key: payload.fact_idempotency_key,
            },
        )
        .await
        .map_err(map_room_progression_error)?;

    Ok(Json(room_progression_fact_response(snapshot)))
}

fn resolved_triggered_by_account_id(
    headers: &HeaderMap,
    triggered_by_kind: &str,
    triggered_by_account_id: &Option<String>,
) -> Result<Option<String>, ApiError> {
    if triggered_by_kind.trim() != "operator" {
        return Ok(triggered_by_account_id.clone());
    }

    let operator_id = parse_uuid_string(
        &require_operator_id(headers)?,
        "x-musubi-operator-id header",
    )?;
    if let Some(body_operator_id) = triggered_by_account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let body_operator_id = parse_uuid_string(body_operator_id, "triggered_by_account_id")?;
        if body_operator_id != operator_id {
            return Err(bad_request(
                "triggered_by_account_id must match x-musubi-operator-id header",
            ));
        }
    }

    Ok(Some(operator_id))
}

fn parse_uuid_string(value: &str, label: &str) -> Result<String, ApiError> {
    Uuid::parse_str(value.trim())
        .map(|uuid| uuid.to_string())
        .map_err(|_| bad_request(format!("{label} must be a valid UUID")))
}

pub async fn get_room_progression_view(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(room_progression_id): Path<String>,
) -> ApiResult<RoomProgressionViewResponse> {
    let token = require_bearer_token(&headers)?;
    let account = authorize_account(&state, &token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = state
        .room_progression
        .get_room_progression_view_for_participant(&account.account_id, room_progression_id.trim())
        .await
        .map_err(map_room_progression_error)?;

    Ok(Json(room_progression_view_response(snapshot)))
}

pub async fn rebuild_room_progression_views(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<RoomProgressionRebuildResponse> {
    require_internal_bearer_token(&headers)?;
    let snapshot = state
        .room_progression
        .rebuild_room_progression_views()
        .await
        .map_err(map_room_progression_error)?;

    Ok(Json(room_progression_rebuild_response(snapshot)))
}

fn room_progression_track_response(
    snapshot: RoomProgressionTrackSnapshot,
) -> RoomProgressionTrackResponse {
    RoomProgressionTrackResponse {
        room_progression_id: snapshot.room_progression_id,
        realm_id: snapshot.realm_id,
        participant_a_account_id: snapshot.participant_a_account_id,
        participant_b_account_id: snapshot.participant_b_account_id,
        related_promise_intent_id: snapshot.related_promise_intent_id,
        related_settlement_case_id: snapshot.related_settlement_case_id,
        current_stage: snapshot.current_stage,
        current_status_code: snapshot.current_status_code,
        current_user_facing_reason_code: snapshot.current_user_facing_reason_code,
        current_review_case_id: snapshot.current_review_case_id,
        source_fact_kind: snapshot.source_fact_kind,
        source_fact_id: snapshot.source_fact_id,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    }
}

fn room_progression_fact_response(
    snapshot: RoomProgressionFactSnapshot,
) -> RoomProgressionFactResponse {
    RoomProgressionFactResponse {
        room_progression_fact_id: snapshot.room_progression_fact_id,
        room_progression_id: snapshot.room_progression_id,
        from_stage: snapshot.from_stage,
        to_stage: snapshot.to_stage,
        transition_kind: snapshot.transition_kind,
        status_code: snapshot.status_code,
        user_facing_reason_code: snapshot.user_facing_reason_code,
        triggered_by_kind: snapshot.triggered_by_kind,
        triggered_by_account_id: snapshot.triggered_by_account_id,
        source_fact_kind: snapshot.source_fact_kind,
        source_fact_id: snapshot.source_fact_id,
        review_case_id: snapshot.review_case_id,
        recorded_at: snapshot.recorded_at,
    }
}

fn room_progression_view_response(
    snapshot: RoomProgressionViewSnapshot,
) -> RoomProgressionViewResponse {
    RoomProgressionViewResponse {
        room_progression_id: snapshot.room_progression_id,
        realm_id: snapshot.realm_id,
        participant_a_account_id: snapshot.participant_a_account_id,
        participant_b_account_id: snapshot.participant_b_account_id,
        visible_stage: snapshot.visible_stage,
        status_code: snapshot.status_code,
        user_facing_reason_code: snapshot.user_facing_reason_code,
        review_case_id: snapshot.review_case_id,
        review_pending: snapshot.review_pending,
        review_status: snapshot.review_status,
        appeal_available: snapshot.appeal_available,
        evidence_requested: snapshot.evidence_requested,
        source_watermark_at: snapshot.source_watermark_at,
        source_fact_count: snapshot.source_fact_count,
        projection_lag_ms: snapshot.projection_lag_ms,
        rebuild_generation: snapshot.rebuild_generation,
        last_projected_at: snapshot.last_projected_at,
    }
}

fn room_progression_rebuild_response(
    snapshot: RoomProgressionRebuildSnapshot,
) -> RoomProgressionRebuildResponse {
    RoomProgressionRebuildResponse {
        rebuilt_count: snapshot.rebuilt_count,
    }
}

fn map_room_progression_error(error: RoomProgressionError) -> ApiError {
    match error {
        RoomProgressionError::BadRequest(message) => bad_request(message),
        RoomProgressionError::Unauthorized(message) => unauthorized(message),
        RoomProgressionError::NotFound(message) => not_found(message),
        RoomProgressionError::Database {
            message, retryable, ..
        } => {
            eprintln!("database room progression error: {message}");
            if retryable {
                service_unavailable("temporarily unavailable")
            } else {
                internal_server_error("internal server error")
            }
        }
        RoomProgressionError::Internal(message) => {
            eprintln!("internal room progression error: {message}");
            internal_server_error("internal server error")
        }
    }
}
