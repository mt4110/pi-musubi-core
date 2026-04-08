use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};

use crate::{
    SharedState,
    handlers::{ApiResult, bad_request, not_found, unauthorized},
    services::happy_route::{
        HappyRouteError, PromiseIntentInput, authorize_account,
        create_promise_intent as create_promise_intent_service,
    },
};

#[derive(Debug, Deserialize)]
pub struct CreatePromiseIntentRequest {
    pub internal_idempotency_key: String,
    pub realm_id: String,
    pub counterparty_account_id: String,
    pub deposit_amount_minor_units: i128,
    pub currency_code: String,
}

#[derive(Debug, Serialize)]
pub struct CreatePromiseIntentResponse {
    pub promise_intent_id: String,
    pub settlement_case_id: String,
    pub case_status: String,
    pub outbox_event_ids: Vec<String>,
    pub replayed_intent: bool,
}

pub async fn create_promise_intent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<CreatePromiseIntentRequest>,
) -> ApiResult<CreatePromiseIntentResponse> {
    let bearer_token = extract_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;

    let outcome = create_promise_intent_service(
        &state,
        &authenticated_account.account_id,
        PromiseIntentInput {
            internal_idempotency_key: payload.internal_idempotency_key.trim().to_owned(),
            realm_id: payload.realm_id.trim().to_owned(),
            counterparty_account_id: payload.counterparty_account_id.trim().to_owned(),
            deposit_amount_minor_units: payload.deposit_amount_minor_units,
            currency_code: payload.currency_code.trim().to_owned(),
        },
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(CreatePromiseIntentResponse {
        promise_intent_id: outcome.promise_intent_id,
        settlement_case_id: outcome.settlement_case_id,
        case_status: outcome.case_status,
        outbox_event_ids: outcome.outbox_event_ids,
        replayed_intent: outcome.replayed_intent,
    }))
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<String, crate::handlers::ApiError> {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| unauthorized("authorization bearer token is required"))?;

    let Some(token) = authorization.strip_prefix("Bearer ") else {
        return Err(unauthorized("authorization bearer token is required"));
    };

    let token = token.trim();
    if token.is_empty() {
        return Err(unauthorized("authorization bearer token is required"));
    }

    Ok(token.to_owned())
}

fn map_happy_route_error(error: HappyRouteError) -> crate::handlers::ApiError {
    match error {
        HappyRouteError::BadRequest(message) => bad_request(message),
        HappyRouteError::Unauthorized(message) => unauthorized(message),
        HappyRouteError::NotFound(message) => not_found(message),
        HappyRouteError::Internal(message) => bad_request(message),
    }
}
