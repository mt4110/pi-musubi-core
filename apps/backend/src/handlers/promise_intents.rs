use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};

use crate::{
    SharedState,
    handlers::{
        ApiResult, bad_request, launch_blocked, map_happy_route_error, require_bearer_token,
    },
    services::{
        happy_route::{
            PromiseIntentInput, authorize_account,
            create_promise_intent as create_promise_intent_service,
        },
        launch_posture::LaunchAction,
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
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    state
        .launch_posture
        .check_participant_action(
            LaunchAction::PromiseCreation,
            &authenticated_account.account_id,
            Some(&authenticated_account.pi_uid),
        )
        .await
        .map_err(|block| launch_blocked(block.status_code, block.message_code))?;
    let internal_idempotency_key = payload.internal_idempotency_key.trim().to_owned();
    if internal_idempotency_key.is_empty() {
        return Err(bad_request("internal_idempotency_key is required"));
    }
    let realm_id = payload.realm_id.trim().to_owned();
    if realm_id.is_empty() {
        return Err(bad_request("realm_id is required"));
    }
    let counterparty_account_id = payload.counterparty_account_id.trim().to_owned();
    if counterparty_account_id.is_empty() {
        return Err(bad_request("counterparty_account_id is required"));
    }
    let currency_code = payload.currency_code.trim().to_owned();
    if currency_code.is_empty() {
        return Err(bad_request("currency_code is required"));
    }

    let outcome = create_promise_intent_service(
        &state,
        &authenticated_account.account_id,
        PromiseIntentInput {
            internal_idempotency_key,
            realm_id,
            counterparty_account_id,
            deposit_amount_minor_units: payload.deposit_amount_minor_units,
            currency_code,
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
