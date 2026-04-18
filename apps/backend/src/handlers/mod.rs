use axum::{
    Json,
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;

use crate::services::happy_route::{HappyRouteError, ProviderErrorClass};

pub mod auth;
pub mod orchestration;
pub mod payments;
pub mod projection;
pub mod promise_intents;
pub mod proof;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

pub type ApiError = (StatusCode, Json<ErrorResponse>);
pub type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn bad_request(message: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

pub fn unauthorized(message: impl Into<String>) -> ApiError {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

pub fn not_found(message: impl Into<String>) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

pub fn internal_server_error(message: impl Into<String>) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

pub fn service_unavailable(message: impl Into<String>) -> ApiError {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}

pub fn require_bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| unauthorized("authorization bearer token is required"))?;

    let mut parts = authorization.split_whitespace();
    let Some(scheme) = parts.next() else {
        return Err(unauthorized("authorization bearer token is required"));
    };
    let Some(token) = parts.next() else {
        return Err(unauthorized("authorization bearer token is required"));
    };
    if !scheme.eq_ignore_ascii_case("bearer") || parts.next().is_some() {
        return Err(unauthorized("authorization bearer token is required"));
    }

    let token = token.trim();
    if token.is_empty() {
        return Err(unauthorized("authorization bearer token is required"));
    }

    Ok(token.to_owned())
}

pub fn map_happy_route_error(error: HappyRouteError) -> ApiError {
    match error {
        HappyRouteError::BadRequest(message) => bad_request(message),
        HappyRouteError::Unauthorized(message) => unauthorized(message),
        HappyRouteError::NotFound(message) => not_found(message),
        HappyRouteError::ProviderCallbackMappingDeferred(message) => {
            eprintln!("provider callback mapping deferred: {message}");
            service_unavailable("provider callback processing deferred")
        }
        HappyRouteError::Provider { class, message } => {
            eprintln!("provider happy route error ({class:?}): {message}");
            match class {
                ProviderErrorClass::Retryable => {
                    service_unavailable("provider temporarily unavailable")
                }
                ProviderErrorClass::Terminal | ProviderErrorClass::ManualReview => {
                    internal_server_error("provider requires review")
                }
            }
        }
        HappyRouteError::Database {
            message, retryable, ..
        } => {
            eprintln!("database happy route error: {message}");
            if retryable {
                service_unavailable("temporarily unavailable")
            } else {
                internal_server_error("internal server error")
            }
        }
        HappyRouteError::Internal(message) => {
            eprintln!("internal happy route error: {message}");
            internal_server_error("internal server error")
        }
    }
}
