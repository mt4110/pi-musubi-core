use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    SharedState,
    handlers::{ApiResult, bad_request},
    services::happy_route::{AuthenticationInput, authenticate_pi_account},
};

#[derive(Debug, Deserialize)]
pub struct PiAuthRequest {
    pub uid: Option<String>,
    pub pi_uid: Option<String>,
    pub username: Option<String>,
    pub wallet_address: Option<String>,
    pub access_token: Option<String>,
    pub profile: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct PiAuthResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub pi_uid: String,
    pub username: String,
}

pub async fn authenticate_pi(
    axum::extract::State(state): axum::extract::State<SharedState>,
    Json(payload): Json<PiAuthRequest>,
) -> ApiResult<PiAuthResponse> {
    let pi_uid = payload
        .pi_uid
        .or(payload.uid)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("pi_uid is required"))?;

    let username = payload
        .username
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("@{pi_uid}"));

    println!(
        "pi auth received: pi_uid={pi_uid}, username={username}, wallet_address={:?}, has_access_token={}, has_profile={}",
        payload.wallet_address,
        payload
            .access_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()),
        payload.profile.is_some(),
    );

    let authenticated = authenticate_pi_account(
        &state,
        AuthenticationInput {
            pi_uid: pi_uid.clone(),
            username: username.clone(),
            wallet_address: payload.wallet_address,
        },
    )
    .await
    .map_err(|error| bad_request(error.message()))?;

    Ok(Json(PiAuthResponse {
        token: authenticated.token,
        user: AuthUser {
            id: authenticated.account_id,
            pi_uid: authenticated.pi_uid,
            username: authenticated.username,
        },
    }))
}
