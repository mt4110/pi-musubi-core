use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};

use crate::{
    SharedState,
    handlers::{ApiResult, bad_request, map_happy_route_error, require_internal_bearer_token},
    services::happy_route::{
        OrchestrationRepairInput, drain_outbox as drain_outbox_service,
        repair_orchestration as repair_orchestration_service,
    },
};

const MAX_REPAIR_ROWS_PER_CATEGORY: i64 = 500;

#[derive(Debug, Serialize)]
pub struct DrainOutboxResponse {
    pub processed_messages: Vec<ProcessedOutboxMessageResponse>,
}

#[derive(Debug, Serialize)]
pub struct ProcessedOutboxMessageResponse {
    pub event_id: String,
    pub event_type: String,
    pub aggregate_id: String,
    pub consumer_name: String,
    pub provider_submission_id: Option<String>,
    pub already_processed: bool,
}

#[derive(Debug, Serialize)]
pub struct OrchestrationRepairResponse {
    pub recovery_run_id: String,
    pub dry_run: bool,
    pub stale_outbox_reclaimed_count: i64,
    pub stale_inbox_reclaimed_count: i64,
    pub producer_cleanup_repaired_count: i64,
    pub callback_ingest_enqueued_count: i64,
    pub verified_receipt_repaired_count: i64,
    pub limited: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrchestrationRepairRequest {
    pub dry_run: Option<bool>,
    pub reason: Option<String>,
    pub max_rows_per_category: Option<i64>,
    pub include_stale_claims: Option<bool>,
    pub include_producer_cleanup: Option<bool>,
    pub include_callback_ingest: Option<bool>,
    pub include_verified_receipt_side_effects: Option<bool>,
}

pub async fn repair_orchestration(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(request): Json<OrchestrationRepairRequest>,
) -> ApiResult<OrchestrationRepairResponse> {
    require_internal_bearer_token(&headers)?;
    let input = repair_input_from_request(request, &headers)?;
    let outcome = repair_orchestration_service(&state, input)
        .await
        .map_err(map_happy_route_error)?;

    Ok(Json(OrchestrationRepairResponse {
        recovery_run_id: outcome.recovery_run_id,
        dry_run: outcome.dry_run,
        stale_outbox_reclaimed_count: outcome.stale_outbox_reclaimed_count,
        stale_inbox_reclaimed_count: outcome.stale_inbox_reclaimed_count,
        producer_cleanup_repaired_count: outcome.producer_cleanup_repaired_count,
        callback_ingest_enqueued_count: outcome.callback_ingest_enqueued_count,
        verified_receipt_repaired_count: outcome.verified_receipt_repaired_count,
        limited: outcome.limited,
    }))
}

fn repair_input_from_request(
    request: OrchestrationRepairRequest,
    headers: &HeaderMap,
) -> Result<OrchestrationRepairInput, crate::handlers::ApiError> {
    let dry_run = request
        .dry_run
        .ok_or_else(|| bad_request("dry_run is required"))?;
    let reason = request
        .reason
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("reason is required"))?;
    let max_rows_per_category = request
        .max_rows_per_category
        .ok_or_else(|| bad_request("max_rows_per_category is required"))?;
    if max_rows_per_category <= 0 {
        return Err(bad_request("max_rows_per_category must be greater than 0"));
    }
    if max_rows_per_category > MAX_REPAIR_ROWS_PER_CATEGORY {
        return Err(bad_request(format!(
            "max_rows_per_category must be at most {MAX_REPAIR_ROWS_PER_CATEGORY}"
        )));
    }

    let include_stale_claims = request
        .include_stale_claims
        .ok_or_else(|| bad_request("include_stale_claims is required"))?;
    let include_producer_cleanup = request
        .include_producer_cleanup
        .ok_or_else(|| bad_request("include_producer_cleanup is required"))?;
    let include_callback_ingest = request
        .include_callback_ingest
        .ok_or_else(|| bad_request("include_callback_ingest is required"))?;
    let include_verified_receipt_side_effects = request
        .include_verified_receipt_side_effects
        .ok_or_else(|| bad_request("include_verified_receipt_side_effects is required"))?;
    if !include_stale_claims
        && !include_producer_cleanup
        && !include_callback_ingest
        && !include_verified_receipt_side_effects
    {
        return Err(bad_request("at least one repair category must be included"));
    }

    let requested_by = headers
        .get("x-musubi-operator-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("internal_orchestration_repair")
        .to_owned();

    Ok(OrchestrationRepairInput {
        dry_run,
        reason,
        requested_by,
        max_rows_per_category,
        include_stale_claims,
        include_producer_cleanup,
        include_callback_ingest,
        include_verified_receipt_side_effects,
    })
}

pub async fn drain_outbox(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<DrainOutboxResponse> {
    require_internal_bearer_token(&headers)?;
    let outcome = drain_outbox_service(&state)
        .await
        .map_err(map_happy_route_error)?;

    Ok(Json(DrainOutboxResponse {
        processed_messages: outcome
            .processed_messages
            .into_iter()
            .map(|message| ProcessedOutboxMessageResponse {
                event_id: message.event_id,
                event_type: message.event_type,
                aggregate_id: message.aggregate_id,
                consumer_name: message.consumer_name,
                provider_submission_id: message.provider_submission_id,
                already_processed: message.already_processed,
            })
            .collect(),
    }))
}
