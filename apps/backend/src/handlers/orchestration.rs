use axum::{Json, extract::State, http::HeaderMap};
use serde::Serialize;

use crate::{
    SharedState,
    handlers::{ApiResult, map_happy_route_error, require_internal_bearer_token},
    services::happy_route::{
        drain_outbox as drain_outbox_service, repair_orchestration as repair_orchestration_service,
    },
};

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
    pub stale_outbox_reclaimed_count: i32,
    pub stale_inbox_reclaimed_count: i32,
    pub producer_cleanup_repaired_count: i32,
    pub callback_ingest_enqueued_count: i32,
    pub verified_receipt_repaired_count: i32,
}

pub async fn repair_orchestration(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<OrchestrationRepairResponse> {
    require_internal_bearer_token(&headers)?;
    let outcome = repair_orchestration_service(&state)
        .await
        .map_err(map_happy_route_error)?;

    Ok(Json(OrchestrationRepairResponse {
        recovery_run_id: outcome.recovery_run_id,
        stale_outbox_reclaimed_count: outcome.stale_outbox_reclaimed_count,
        stale_inbox_reclaimed_count: outcome.stale_inbox_reclaimed_count,
        producer_cleanup_repaired_count: outcome.producer_cleanup_repaired_count,
        callback_ingest_enqueued_count: outcome.callback_ingest_enqueued_count,
        verified_receipt_repaired_count: outcome.verified_receipt_repaired_count,
    }))
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
