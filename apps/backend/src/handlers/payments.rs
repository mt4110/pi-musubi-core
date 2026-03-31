use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{
    SharedState,
    handlers::{ApiResult, bad_request},
    services::escrow::{EscrowFundingInput, fund_escrow},
};

#[derive(Debug, Deserialize)]
pub struct PaymentCallbackRequest {
    pub payment_id: String,
    pub payer_pi_uid: String,
    pub target_user_id: String,
    pub amount_pi: f64,
    pub txid: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaymentCallbackResponse {
    pub payment_id: String,
    pub target_user_id: String,
    pub escrow_status: &'static str,
    pub message: String,
}

pub async fn payment_callback(
    State(state): State<SharedState>,
    Json(payload): Json<PaymentCallbackRequest>,
) -> ApiResult<PaymentCallbackResponse> {
    let payment_id = payload.payment_id.trim().to_owned();
    if payment_id.is_empty() {
        return Err(bad_request("payment_id is required"));
    }

    let payer_pi_uid = payload.payer_pi_uid.trim().to_owned();
    if payer_pi_uid.is_empty() {
        return Err(bad_request("payer_pi_uid is required"));
    }

    let target_user_id = payload.target_user_id.trim().to_owned();
    if target_user_id.is_empty() {
        return Err(bad_request("target_user_id is required"));
    }

    if payload.amount_pi <= 0.0 {
        return Err(bad_request("amount_pi must be greater than zero"));
    }

    let funded = fund_escrow(
        &state,
        EscrowFundingInput {
            payment_id,
            payer_pi_uid,
            target_user_id,
            amount_pi: payload.amount_pi,
            txid: payload.txid,
            callback_status: payload.status.unwrap_or_else(|| "completed".to_owned()),
        },
    )
    .await;

    Ok(Json(PaymentCallbackResponse {
        payment_id: funded.payment_id.clone(),
        target_user_id: funded.target_user_id.clone(),
        escrow_status: funded.status.as_str(),
        message: format!(
            "Escrow for user {} is now {}.",
            funded.target_user_id,
            funded.status.as_str()
        ),
    }))
}
