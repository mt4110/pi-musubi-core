use axum::{
    Json,
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;

use crate::services::{
    happy_route::{HappyRouteError, ProviderErrorClass},
    launch_posture::{LaunchBlock, LaunchBlockKind},
};

pub mod auth;
pub mod launch_posture;
pub mod operator_review;
pub mod ops_observability;
pub mod orchestration;
pub mod payments;
pub mod projection;
pub mod promise_intents;
pub mod proof;
pub mod realm_bootstrap;
pub mod room_progression;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_code: Option<String>,
}

pub type ApiError = (StatusCode, Json<ErrorResponse>);
pub type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn bad_request(message: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: message.into(),
            message_code: None,
        }),
    )
}

pub fn unauthorized(message: impl Into<String>) -> ApiError {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: message.into(),
            message_code: None,
        }),
    )
}

pub fn conflict(message: impl Into<String>) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse {
            error: message.into(),
            message_code: None,
        }),
    )
}

pub fn not_found(message: impl Into<String>) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: message.into(),
            message_code: None,
        }),
    )
}

pub fn internal_server_error(message: impl Into<String>) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: message.into(),
            message_code: None,
        }),
    )
}

pub fn service_unavailable(message: impl Into<String>) -> ApiError {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: message.into(),
            message_code: None,
        }),
    )
}

pub fn launch_blocked(status: StatusCode, message_code: impl Into<String>) -> ApiError {
    let message_code = message_code.into();
    (
        status,
        Json(ErrorResponse {
            error: message_code.clone(),
            message_code: Some(message_code),
        }),
    )
}

pub fn launch_blocked_from_service(block: LaunchBlock) -> ApiError {
    let status = match block.kind {
        LaunchBlockKind::Forbidden => StatusCode::FORBIDDEN,
        LaunchBlockKind::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    launch_blocked(status, block.message_code)
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

pub fn require_operator_id(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("x-musubi-operator-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| bad_request("x-musubi-operator-id header is required"))
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
        HappyRouteError::Conflict(message) => conflict(message),
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

    use super::{HeaderMap, require_internal_bearer_token_with_config, require_operator_id};

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

    #[test]
    fn operator_id_header_is_required_and_trimmed() {
        let headers = HeaderMap::new();
        assert!(require_operator_id(&headers).is_err());

        let mut blank_headers = HeaderMap::new();
        blank_headers.insert("x-musubi-operator-id", HeaderValue::from_static("  "));
        assert!(require_operator_id(&blank_headers).is_err());

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-musubi-operator-id",
            HeaderValue::from_static(" 123e4567-e89b-12d3-a456-426614174000 "),
        );
        assert_eq!(
            require_operator_id(&headers).expect("operator id should parse"),
            "123e4567-e89b-12d3-a456-426614174000"
        );
    }
}
