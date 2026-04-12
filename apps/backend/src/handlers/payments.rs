use axum::{Json, body::Bytes, extract::State, http::HeaderMap};
use serde::Serialize;

use crate::{
    SharedState,
    handlers::{ApiResult, map_happy_route_error},
    services::happy_route::{PaymentCallbackInput, accept_payment_callback},
};

#[derive(Debug, Serialize)]
pub struct PaymentCallbackResponse {
    pub raw_callback_id: String,
    pub outbox_event_ids: Vec<String>,
    pub duplicate_callback: bool,
}

pub async fn payment_callback(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<PaymentCallbackResponse> {
    let outcome = accept_payment_callback(
        &state,
        PaymentCallbackInput {
            raw_body_bytes: body.to_vec(),
            redacted_headers: redacted_headers(&headers),
        },
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(PaymentCallbackResponse {
        raw_callback_id: outcome.raw_callback_id,
        outbox_event_ids: outcome.outbox_event_ids,
        duplicate_callback: outcome.duplicate_callback,
    }))
}

fn redacted_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    let mut values = headers
        .iter()
        .map(|(name, value)| {
            let header_name = name.as_str().to_ascii_lowercase();
            let header_value = if is_sensitive_header(&header_name) {
                "[redacted]".to_owned()
            } else {
                value.to_str().unwrap_or("[non-utf8]").to_owned()
            };
            (header_name, header_value)
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.0.cmp(&right.0));
    values
}

fn is_sensitive_header(name: &str) -> bool {
    name == "authorization"
        || name == "cookie"
        || name == "set-cookie"
        || name.contains("api-key")
        || name.contains("secret")
        || name.contains("token")
}
