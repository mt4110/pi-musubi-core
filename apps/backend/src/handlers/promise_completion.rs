use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use crate::{
    SharedState,
    handlers::{
        ApiError, ApiResult, bad_request, conflict, internal_server_error, require_bearer_token,
        service_unavailable,
    },
    services::{
        happy_route::authorize_account,
        promise_completion::PromiseCompletionWriterFactPersistenceError,
    },
};

#[derive(Debug, Deserialize)]
pub struct ParticipantSafeDisplayAvailabilityQuery {
    pub realm_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParticipantSafeDisplayAvailabilityResponse {
    pub display_availability: String,
    pub completed_reference_available: bool,
}

pub async fn get_participant_safe_display_availability(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(promise_reference): Path<String>,
    Query(query): Query<ParticipantSafeDisplayAvailabilityQuery>,
) -> ApiResult<ParticipantSafeDisplayAvailabilityResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let account = authorize_account(&state, &bearer_token)
        .await
        .map_err(super::map_happy_route_error)?;
    let Some(realm_id) = query
        .realm_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(Json(participant_safe_display_availability_response(false)));
    };

    let availability = state
        .promise_completion
        .derive_participant_safe_completed_reference_display_availability_for_account(
            promise_reference.trim(),
            realm_id,
            &account.account_id,
        )
        .await
        .map_err(map_promise_completion_error)?;

    Ok(Json(participant_safe_display_availability_response(
        availability.is_some(),
    )))
}

fn participant_safe_display_availability_response(
    available: bool,
) -> ParticipantSafeDisplayAvailabilityResponse {
    if available {
        return ParticipantSafeDisplayAvailabilityResponse {
            display_availability: "available".to_owned(),
            completed_reference_available: true,
        };
    }

    ParticipantSafeDisplayAvailabilityResponse {
        display_availability: "unavailable".to_owned(),
        completed_reference_available: false,
    }
}

fn map_promise_completion_error(error: PromiseCompletionWriterFactPersistenceError) -> ApiError {
    match error {
        PromiseCompletionWriterFactPersistenceError::BadRequest(message) => bad_request(message),
        PromiseCompletionWriterFactPersistenceError::IdempotencyConflict { message, .. } => {
            conflict(message)
        }
        PromiseCompletionWriterFactPersistenceError::Database {
            message, retryable, ..
        } => {
            eprintln!("database Promise completion display availability error: {message}");
            if retryable {
                service_unavailable("temporarily unavailable")
            } else {
                internal_server_error("internal server error")
            }
        }
        PromiseCompletionWriterFactPersistenceError::Internal(message) => {
            eprintln!("internal Promise completion display availability error: {message}");
            internal_server_error("internal server error")
        }
    }
}
