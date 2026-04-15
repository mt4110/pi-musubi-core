use axum::{Json, extract::State, http::HeaderMap};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    SharedState,
    handlers::{ApiResult, bad_request, map_happy_route_error, require_bearer_token},
    services::{
        happy_route::authorize_account,
        proof::{
            ProofEnvelopeInput, ProofSubmissionOutcome, StartProofChallengeInput,
            start_proof_challenge as start_proof_challenge_service,
            submit_proof_envelope as submit_proof_envelope_service,
        },
    },
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartProofChallengeRequest {
    pub venue_id: String,
    pub realm_id: String,
    pub fallback_mode: Option<String>,
    pub operator_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StartProofChallengeResponse {
    pub challenge_id: String,
    pub venue_id: String,
    pub realm_id: String,
    pub expires_at: DateTime<Utc>,
    pub client_nonce: String,
    pub allowed_fallback_mode: String,
    pub venue_key_version: i32,
    pub operator_pin_issued: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitProofEnvelopeRequest {
    pub challenge_id: Option<String>,
    pub venue_id: Option<String>,
    pub display_code: Option<String>,
    pub key_version: Option<i32>,
    pub client_nonce: Option<String>,
    pub observed_at_ms: Option<i64>,
    pub coarse_location_bucket: Option<String>,
    pub device_session_id: Option<String>,
    pub fallback_mode: Option<String>,
    pub operator_pin: Option<String>,
}

pub async fn start_proof_challenge(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<StartProofChallengeRequest>,
) -> ApiResult<StartProofChallengeResponse> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;
    if public_fallback_mode(&payload.fallback_mode)? != "none" {
        return Err(bad_request(
            "operator_pin fallback is not available from the public proof challenge endpoint",
        ));
    }

    let outcome = start_proof_challenge_service(
        &state,
        StartProofChallengeInput {
            subject_account_id: authenticated_account.account_id,
            venue_id: payload.venue_id,
            realm_id: payload.realm_id,
            fallback_mode: "none".to_owned(),
            operator_id: None,
        },
    )
    .await
    .map_err(map_happy_route_error)?;
    let client_outcome = outcome.client;

    Ok(Json(StartProofChallengeResponse {
        challenge_id: client_outcome.challenge_id,
        venue_id: client_outcome.venue_id,
        realm_id: client_outcome.realm_id,
        expires_at: client_outcome.expires_at,
        client_nonce: client_outcome.client_nonce,
        allowed_fallback_mode: client_outcome.allowed_fallback_mode,
        venue_key_version: client_outcome.venue_key_version,
        operator_pin_issued: client_outcome.operator_pin_issued,
    }))
}

pub async fn submit_proof_envelope(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<SubmitProofEnvelopeRequest>,
) -> ApiResult<ProofSubmissionOutcome> {
    let bearer_token = require_bearer_token(&headers)?;
    let authenticated_account = authorize_account(&state, &bearer_token)
        .await
        .map_err(map_happy_route_error)?;

    let outcome = submit_proof_envelope_service(
        &state,
        ProofEnvelopeInput {
            subject_account_id: authenticated_account.account_id,
            challenge_id: payload.challenge_id,
            venue_id: payload.venue_id,
            display_code: payload.display_code,
            key_version: payload.key_version,
            client_nonce: payload.client_nonce,
            observed_at_ms: payload.observed_at_ms,
            coarse_location_bucket: payload.coarse_location_bucket,
            device_session_id: payload.device_session_id,
            fallback_mode: payload.fallback_mode,
            operator_pin: payload.operator_pin,
        },
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(outcome))
}

fn public_fallback_mode(value: &Option<String>) -> Result<&'static str, crate::handlers::ApiError> {
    let normalized = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none");
    if normalized == "none" {
        Ok("none")
    } else if normalized == "operator_pin" {
        Ok("operator_pin")
    } else {
        Err(bad_request(
            "fallback_mode must be none for public proof challenges",
        ))
    }
}
