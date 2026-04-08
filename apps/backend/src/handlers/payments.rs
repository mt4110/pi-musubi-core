use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{
    SharedState,
    handlers::{ApiResult, bad_request, map_happy_route_error},
    services::happy_route::{PaymentCallbackInput, ingest_payment_callback},
};

#[derive(Debug, Deserialize)]
pub struct PaymentCallbackRequest {
    pub payment_id: String,
    pub payer_pi_uid: String,
    pub amount_minor_units: i128,
    pub currency_code: String,
    pub txid: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaymentCallbackResponse {
    pub payment_receipt_id: String,
    pub raw_callback_id: String,
    pub settlement_case_id: String,
    pub promise_intent_id: String,
    pub case_status: String,
    pub receipt_status: String,
    pub ledger_journal_id: Option<String>,
    pub outbox_event_ids: Vec<String>,
    pub duplicate_receipt: bool,
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

    let currency_code = payload.currency_code.trim().to_owned();
    if currency_code.is_empty() {
        return Err(bad_request("currency_code is required"));
    }

    let outcome = ingest_payment_callback(
        &state,
        PaymentCallbackInput {
            payment_id,
            payer_pi_uid,
            amount_minor_units: payload.amount_minor_units,
            currency_code,
            txid: payload.txid,
            callback_status: payload.status.unwrap_or_else(|| "completed".to_owned()),
        },
    )
    .await
    .map_err(map_happy_route_error)?;

    Ok(Json(PaymentCallbackResponse {
        payment_receipt_id: outcome.payment_receipt_id,
        raw_callback_id: outcome.raw_callback_id,
        settlement_case_id: outcome.settlement_case_id,
        promise_intent_id: outcome.promise_intent_id,
        case_status: outcome.case_status,
        receipt_status: outcome.receipt_status,
        ledger_journal_id: outcome.ledger_journal_id,
        outbox_event_ids: outcome.outbox_event_ids,
        duplicate_receipt: outcome.duplicate_receipt,
    }))
}
