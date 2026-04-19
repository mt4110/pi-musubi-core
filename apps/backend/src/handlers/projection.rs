use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    SharedState,
    handlers::{
        ApiResult, map_happy_route_error, require_bearer_token, require_internal_bearer_token,
    },
    services::happy_route::{
        ExpandedSettlementViewSnapshot, ProjectionProvenance, PromiseProjectionSnapshot,
        TrustSnapshot, authorize_account,
        get_expanded_settlement_view as get_expanded_settlement_view_service,
        get_promise_projection as get_promise_projection_service,
        get_realm_trust_snapshot as get_realm_trust_snapshot_service,
        get_settlement_view as get_settlement_view_service,
        get_trust_snapshot as get_trust_snapshot_service,
        rebuild_projection_read_models as rebuild_projection_read_models_service,
    },
};

#[derive(Debug, Serialize)]
pub struct SettlementViewResponse {
    pub settlement_case_id: String,
    pub promise_intent_id: String,
    pub realm_id: String,
    pub current_settlement_status: String,
    pub total_funded_minor_units: i128,
    pub currency_code: String,
    pub latest_journal_entry_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectionProvenanceResponse {
    pub source_watermark_at: DateTime<Utc>,
    pub source_fact_count: i64,
    pub freshness_checked_at: DateTime<Utc>,
    pub projection_lag_ms: i64,
    pub last_projected_at: DateTime<Utc>,
    pub rebuild_generation: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PromiseProjectionResponse {
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
    pub provenance: ProjectionProvenanceResponse,
}

#[derive(Debug, Serialize)]
pub struct ExpandedSettlementViewResponse {
    pub settlement_case_id: String,
    pub promise_intent_id: String,
    pub realm_id: String,
    pub current_settlement_status: String,
    pub total_funded_minor_units: i128,
    pub currency_code: String,
    pub latest_journal_entry_id: Option<String>,
    pub proof_status: String,
    pub proof_signal_count: i64,
    pub provenance: ProjectionProvenanceResponse,
}

#[derive(Debug, Serialize)]
pub struct TrustSnapshotResponse {
    pub account_id: String,
    pub realm_id: Option<String>,
    pub trust_posture: String,
    pub reason_codes: Vec<String>,
    pub promise_participation_count_90d: i64,
    pub funded_settlement_count_90d: i64,
    pub manual_review_case_bucket: String,
    pub proof_status: String,
    pub proof_signal_count: i64,
    pub provenance: ProjectionProvenanceResponse,
}

#[derive(Debug, Serialize)]
pub struct ProjectionRebuildResponse {
    pub rebuild_generation: String,
    pub rebuilt_at: DateTime<Utc>,
    pub rebuilt: Vec<ProjectionRebuildItemResponse>,
}

#[derive(Debug, Serialize)]
pub struct ProjectionRebuildItemResponse {
    pub projection_name: String,
    pub projection_row_count: i64,
    pub source_fact_count: i64,
    pub source_watermark_at: DateTime<Utc>,
    pub projection_lag_ms: i64,
}

pub async fn get_settlement_view(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(settlement_case_id): Path<String>,
) -> ApiResult<SettlementViewResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = get_settlement_view_service(
        &state,
        settlement_case_id.trim(),
        &authenticated_account.account_id,
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(SettlementViewResponse {
        settlement_case_id: snapshot.settlement_case_id,
        promise_intent_id: snapshot.promise_intent_id,
        realm_id: snapshot.realm_id,
        current_settlement_status: snapshot.current_settlement_status,
        total_funded_minor_units: snapshot.total_funded_minor_units,
        currency_code: snapshot.currency_code,
        latest_journal_entry_id: snapshot.latest_journal_entry_id,
    }))
}

pub async fn get_promise_projection(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(promise_intent_id): Path<String>,
) -> ApiResult<PromiseProjectionResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = get_promise_projection_service(
        &state,
        promise_intent_id.trim(),
        &authenticated_account.account_id,
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(promise_projection_response(snapshot)))
}

pub async fn get_expanded_settlement_view(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(settlement_case_id): Path<String>,
) -> ApiResult<ExpandedSettlementViewResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = get_expanded_settlement_view_service(
        &state,
        settlement_case_id.trim(),
        &authenticated_account.account_id,
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(expanded_settlement_response(snapshot)))
}

pub async fn get_trust_snapshot(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(account_id): Path<String>,
) -> ApiResult<TrustSnapshotResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot =
        get_trust_snapshot_service(&state, account_id.trim(), &authenticated_account.account_id)
            .await
            .map_err(map_happy_route_error)?;

    Ok(Json(trust_snapshot_response(snapshot)))
}

pub async fn get_realm_trust_snapshot(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((realm_id, account_id)): Path<(String, String)>,
) -> ApiResult<TrustSnapshotResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    let snapshot = get_realm_trust_snapshot_service(
        &state,
        realm_id.trim(),
        account_id.trim(),
        &authenticated_account.account_id,
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(trust_snapshot_response(snapshot)))
}

pub async fn rebuild_projection_read_models(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<ProjectionRebuildResponse> {
    require_internal_bearer_token(&headers)?;
    let outcome = rebuild_projection_read_models_service(&state)
        .await
        .map_err(map_happy_route_error)?;

    Ok(Json(ProjectionRebuildResponse {
        rebuild_generation: outcome.rebuild_generation,
        rebuilt_at: outcome.rebuilt_at,
        rebuilt: outcome
            .rebuilt
            .into_iter()
            .map(|item| ProjectionRebuildItemResponse {
                projection_name: item.projection_name,
                projection_row_count: item.projection_row_count,
                source_fact_count: item.source_fact_count,
                source_watermark_at: item.source_watermark_at,
                projection_lag_ms: item.projection_lag_ms,
            })
            .collect(),
    }))
}

fn promise_projection_response(snapshot: PromiseProjectionSnapshot) -> PromiseProjectionResponse {
    PromiseProjectionResponse {
        promise_intent_id: snapshot.promise_intent_id,
        realm_id: snapshot.realm_id,
        initiator_account_id: snapshot.initiator_account_id,
        counterparty_account_id: snapshot.counterparty_account_id,
        current_intent_status: snapshot.current_intent_status,
        deposit_amount_minor_units: snapshot.deposit_amount_minor_units,
        currency_code: snapshot.currency_code,
        deposit_scale: snapshot.deposit_scale,
        latest_settlement_case_id: snapshot.latest_settlement_case_id,
        latest_settlement_status: snapshot.latest_settlement_status,
        provenance: provenance_response(snapshot.provenance),
    }
}

fn expanded_settlement_response(
    snapshot: ExpandedSettlementViewSnapshot,
) -> ExpandedSettlementViewResponse {
    ExpandedSettlementViewResponse {
        settlement_case_id: snapshot.settlement_case_id,
        promise_intent_id: snapshot.promise_intent_id,
        realm_id: snapshot.realm_id,
        current_settlement_status: snapshot.current_settlement_status,
        total_funded_minor_units: snapshot.total_funded_minor_units,
        currency_code: snapshot.currency_code,
        latest_journal_entry_id: snapshot.latest_journal_entry_id,
        proof_status: snapshot.proof_status,
        proof_signal_count: snapshot.proof_signal_count,
        provenance: provenance_response(snapshot.provenance),
    }
}

fn trust_snapshot_response(snapshot: TrustSnapshot) -> TrustSnapshotResponse {
    TrustSnapshotResponse {
        account_id: snapshot.account_id,
        realm_id: snapshot.realm_id,
        trust_posture: snapshot.trust_posture,
        reason_codes: snapshot.reason_codes,
        promise_participation_count_90d: snapshot.promise_participation_count_90d,
        funded_settlement_count_90d: snapshot.funded_settlement_count_90d,
        manual_review_case_bucket: snapshot.manual_review_case_bucket,
        proof_status: snapshot.proof_status,
        proof_signal_count: snapshot.proof_signal_count,
        provenance: provenance_response(snapshot.provenance),
    }
}

fn provenance_response(provenance: ProjectionProvenance) -> ProjectionProvenanceResponse {
    ProjectionProvenanceResponse {
        source_watermark_at: provenance.source_watermark_at,
        source_fact_count: provenance.source_fact_count,
        freshness_checked_at: provenance.freshness_checked_at,
        projection_lag_ms: provenance.projection_lag_ms,
        last_projected_at: provenance.last_projected_at,
        rebuild_generation: provenance.rebuild_generation,
    }
}
