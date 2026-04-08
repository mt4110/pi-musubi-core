use axum::{Json, http::StatusCode};
use serde::Serialize;

use crate::services::happy_route::HappyRouteError;

pub mod auth;
pub mod orchestration;
pub mod payments;
pub mod projection;
pub mod promise_intents;

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

pub fn map_happy_route_error(error: HappyRouteError) -> ApiError {
    match error {
        HappyRouteError::BadRequest(message) => bad_request(message),
        HappyRouteError::Unauthorized(message) => unauthorized(message),
        HappyRouteError::NotFound(message) => not_found(message),
        HappyRouteError::Internal(message) => {
            eprintln!("internal happy route error: {message}");
            internal_server_error("internal server error")
        }
    }
}
