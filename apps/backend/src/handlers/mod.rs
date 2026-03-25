use axum::{Json, http::StatusCode};
use serde::Serialize;

pub mod auth;
pub mod payments;

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
