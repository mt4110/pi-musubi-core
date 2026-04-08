use axum::{Json, extract::State};
use serde::Serialize;

use crate::{
    SharedState,
    handlers::{ApiResult, map_happy_route_error},
    services::happy_route::drain_outbox as drain_outbox_service,
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

pub async fn drain_outbox(State(state): State<SharedState>) -> ApiResult<DrainOutboxResponse> {
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
