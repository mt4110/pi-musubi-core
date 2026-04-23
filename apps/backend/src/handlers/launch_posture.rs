use axum::{Json, extract::State, http::HeaderMap};

use crate::{
    SharedState,
    handlers::{
        ApiError, ApiResult, require_bearer_token, require_internal_bearer_token, unauthorized,
    },
    services::launch_posture::{InternalLaunchPostureSnapshot, LaunchPostureSnapshot},
};

pub async fn get_public_launch_posture(
    State(state): State<SharedState>,
) -> ApiResult<LaunchPostureSnapshot> {
    Ok(Json(state.launch_posture.public_snapshot().await))
}

pub async fn get_internal_launch_posture(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<InternalLaunchPostureSnapshot> {
    require_launch_internal_access(&headers)?;
    Ok(Json(state.launch_posture.internal_snapshot().await))
}

fn require_launch_internal_access(headers: &HeaderMap) -> Result<(), ApiError> {
    let has_authorization = headers.contains_key(axum::http::header::AUTHORIZATION);
    if has_authorization {
        let token = require_bearer_token(headers)
            .map_err(|_| unauthorized("internal authorization bearer token is required"))?;
        if let Some(configured_token) = std::env::var("MUSUBI_INTERNAL_API_TOKEN")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            if token == configured_token {
                return Ok(());
            }
        }
        return Err(unauthorized(
            "participant bearer tokens cannot access internal launch posture",
        ));
    }

    require_internal_bearer_token(headers)
}
