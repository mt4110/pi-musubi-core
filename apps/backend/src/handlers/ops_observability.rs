use axum::{Json, extract::State, http::HeaderMap};

use crate::{
    SharedState,
    handlers::{
        ApiError, ApiResult, internal_server_error, require_bearer_token,
        require_internal_bearer_token, service_unavailable, unauthorized,
    },
    services::ops_observability::{
        OpsHealthSnapshot, OpsObservabilityError, OpsObservabilitySnapshot, OpsReadinessSnapshot,
    },
};

pub async fn get_ops_health(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<OpsHealthSnapshot> {
    require_ops_internal_access(&headers)?;
    state
        .ops_observability
        .health()
        .await
        .map(Json)
        .map_err(map_ops_observability_error)
}

pub async fn get_ops_readiness(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<OpsReadinessSnapshot> {
    require_ops_internal_access(&headers)?;
    state
        .ops_observability
        .readiness()
        .await
        .map(Json)
        .map_err(map_ops_observability_error)
}

pub async fn get_ops_snapshot(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<OpsObservabilitySnapshot> {
    require_ops_internal_access(&headers)?;
    state
        .ops_observability
        .snapshot()
        .await
        .map(Json)
        .map_err(map_ops_observability_error)
}

pub async fn get_ops_slo(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<OpsObservabilitySnapshot> {
    require_ops_internal_access(&headers)?;
    state
        .ops_observability
        .snapshot()
        .await
        .map(Json)
        .map_err(map_ops_observability_error)
}

fn require_ops_internal_access(headers: &HeaderMap) -> Result<(), ApiError> {
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
            "internal authorization bearer token is required",
        ));
    }

    require_internal_bearer_token(headers)
}

fn map_ops_observability_error(error: OpsObservabilityError) -> ApiError {
    match error {
        OpsObservabilityError::Database {
            message, retryable, ..
        } => {
            eprintln!("ops observability database error: {message}");
            if retryable {
                service_unavailable("ops observability temporarily unavailable")
            } else {
                internal_server_error("internal server error")
            }
        }
        OpsObservabilityError::Internal(message) => {
            eprintln!("ops observability internal error: {message}");
            internal_server_error("internal server error")
        }
    }
}
