use axum::{
    Json,
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;

use crate::services::happy_route::{HappyRouteError, ProviderErrorClass};

pub mod auth;
pub mod operator_review;
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

pub fn require_internal_bearer_token(headers: &HeaderMap) -> Result<(), ApiError> {
    let configured_token = std::env::var("MUSUBI_INTERNAL_API_TOKEN").ok();
    require_internal_bearer_token_with_config(
        headers,
        cfg!(debug_assertions),
        configured_token.as_deref(),
    )
}

fn require_internal_bearer_token_with_config(
    headers: &HeaderMap,
    debug_build: bool,
    configured_token: Option<&str>,
) -> Result<(), ApiError> {
    if debug_build {
        return Ok(());
    }

    let Some(configured_token) = configured_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
    else {
        return Err(unauthorized(
            "internal authorization bearer token is required",
        ));
    };

    let token = require_bearer_token(headers)
        .map_err(|_| unauthorized("internal authorization bearer token is required"))?;
    if token != configured_token {
        return Err(unauthorized(
            "internal authorization bearer token is required",
        ));
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, header::AUTHORIZATION};

    use super::{HeaderMap, require_internal_bearer_token_with_config};

    #[test]
    fn debug_build_internal_requests_do_not_require_token() {
        let headers = HeaderMap::new();
        assert!(require_internal_bearer_token_with_config(&headers, true, None).is_ok());
    }

    #[test]
    fn release_build_internal_requests_require_matching_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer musubi-internal"),
        );

        assert!(
            require_internal_bearer_token_with_config(&headers, false, Some("musubi-internal"))
                .is_ok()
        );
    }

    #[test]
    fn release_build_internal_requests_reject_wrong_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );

        assert!(
            require_internal_bearer_token_with_config(&headers, false, Some("musubi-internal"))
                .is_err()
        );
    }
}
