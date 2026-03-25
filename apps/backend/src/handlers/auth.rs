use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::handlers::{ApiResult, bad_request};

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

pub async fn authenticate_pi(Json(payload): Json<PiAuthRequest>) -> ApiResult<PiAuthResponse> {
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

    Ok(Json(PiAuthResponse {
        token: format!("pi-session-{}", Uuid::new_v4()),
        user: AuthUser {
            id: Uuid::new_v4().to_string(),
            pi_uid,
            username,
        },
    }))
}
