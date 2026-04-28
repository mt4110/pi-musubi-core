use chrono::Utc;
use musubi_settlement_domain::{
    NormalizeCallbackCmd, PaymentReceiptId, ProviderCallbackId, SettlementBackend,
    VerifyReceiptCmd, VerifyReceiptExpectation,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::SharedState;

use super::{
    backend::PiSettlementBackend,
    common::{canonical_pi_money, map_backend_error},
    constants::PROVIDER_CALLBACK_CONSUMER,
    state::OutboxMessageRecord,
    types::{
        HappyRouteError, ParsedPaymentCallback, PaymentCallbackAccepted, PaymentCallbackInput,
        PaymentCallbackOutcome, ProcessedOutboxMessage, RawPaymentCallbackFields,
        SettlementViewSnapshot, processed_outbox_message,
    },
};

pub async fn accept_payment_callback(
    state: &SharedState,
    input: PaymentCallbackInput,
) -> Result<PaymentCallbackAccepted, HappyRouteError> {
    let raw_callback_id = Uuid::new_v4().to_string();
    let received_at = Utc::now();
    let parsed_payload = parse_callback_payload(&input.raw_body_bytes);
    let raw_fields = parsed_payload
        .as_ref()
        .ok()
        .map(PaymentCallbackPayload::raw_fields);

    let raw_callback_uuid = Uuid::parse_str(&raw_callback_id).map_err(|_| {
        HappyRouteError::Internal("generated raw callback id was not a UUID".to_owned())
    })?;
    let (duplicate_callback, outbox_event_id) = state
        .happy_route
        .accept_payment_callback(&input, raw_fields.as_ref(), raw_callback_uuid, received_at)
        .await?;

    Ok(PaymentCallbackAccepted {
        raw_callback_id,
        duplicate_callback,
        outbox_event_ids: vec![outbox_event_id.to_string()],
    })
}

pub(super) async fn process_provider_callback(
    state: &SharedState,
    message: OutboxMessageRecord,
    raw_callback_id: String,
) -> Result<ProcessedOutboxMessage, HappyRouteError> {
    let raw_callback = state
        .happy_route
        .load_raw_callback(&raw_callback_id)
        .await?;

    let parsed_payload =
        parse_callback_payload(&raw_callback.raw_body_bytes).map_err(callback_review_error)?;
    let parsed =
        ParsedPaymentCallback::try_from_payload(parsed_payload).map_err(callback_review_error)?;
    let observed_amount = canonical_pi_money(parsed.amount_minor_units, &parsed.currency_code)
        .map_err(callback_review_error)?;
    let duplicate_callback = raw_callback.replay_of_raw_callback_id.is_some();

    state
        .happy_route
        .attach_callback_amount(&raw_callback_id, &observed_amount)
        .await?;

    let callback_context = state
        .happy_route
        .load_callback_context(
            &parsed,
            &observed_amount,
            &raw_callback_id,
            duplicate_callback,
        )
        .await
        .map_err(callback_review_error)?;

    if callback_context.duplicate_callback {
        if let Some(outcome) = state
            .happy_route
            .payment_callback_replay_outcome(&callback_context)
            .await?
        {
            state
                .happy_route
                .finalize_provider_callback_replay(&message, &outcome)
                .await?;
            return Ok(processed_provider_callback_message(
                &message,
                &callback_context.provider_submission_id,
                &outcome,
            ));
        }
    }

    let backend = PiSettlementBackend::new(state.clone());
    let normalized_observations = backend
        .normalize_callback(NormalizeCallbackCmd {
            backend: callback_context.settlement_case.backend_pin.clone(),
            raw_callback_ref: ProviderCallbackId::new(callback_context.raw_callback_id.clone()),
        })
        .await
        .map_err(map_backend_error)?;
    let verification = backend
        .verify_receipt(VerifyReceiptCmd {
            backend: callback_context.settlement_case.backend_pin.clone(),
            receipt_id: PaymentReceiptId::new(format!("receipt-{}", parsed.provider_submission_id)),
            raw_callback_ref: ProviderCallbackId::new(callback_context.raw_callback_id.clone()),
            expected: Some(VerifyReceiptExpectation::Amount(
                callback_context.promise_intent.deposit_amount.clone(),
            )),
        })
        .await
        .map_err(map_backend_error)?;

    let outcome = state
        .happy_route
        .persist_payment_callback_result(
            &message,
            &callback_context,
            observed_amount,
            verification,
            normalized_observations,
        )
        .await?;
    Ok(processed_provider_callback_message(
        &message,
        &callback_context.provider_submission_id,
        &outcome,
    ))
}

fn processed_provider_callback_message(
    message: &OutboxMessageRecord,
    provider_submission_id: &str,
    outcome: &PaymentCallbackOutcome,
) -> ProcessedOutboxMessage {
    processed_outbox_message(
        message,
        PROVIDER_CALLBACK_CONSUMER,
        Some(provider_submission_id.to_owned()),
        outcome.duplicate_receipt,
    )
}

fn callback_review_error(error: HappyRouteError) -> HappyRouteError {
    match error {
        HappyRouteError::Provider { .. }
        | HappyRouteError::ProviderCallbackMappingDeferred(_)
        | HappyRouteError::Conflict(_)
        | HappyRouteError::Database { .. }
        | HappyRouteError::Internal(_) => error,
        other => HappyRouteError::Provider {
            class: super::types::ProviderErrorClass::ManualReview,
            message: other.message().to_owned(),
        },
    }
}

#[derive(Clone, Debug, Deserialize)]
struct PaymentCallbackPayload {
    payment_id: Option<String>,
    payer_pi_uid: Option<String>,
    amount_minor_units: Option<i128>,
    currency_code: Option<String>,
    txid: Option<String>,
    status: Option<String>,
}

impl ParsedPaymentCallback {
    fn try_from_payload(payload: PaymentCallbackPayload) -> Result<Self, HappyRouteError> {
        let payment_id = required_trimmed(payload.payment_id, "payment_id is required")?;
        let payer_pi_uid = required_trimmed(payload.payer_pi_uid, "payer_pi_uid is required")?;
        let currency_code = required_trimmed(payload.currency_code, "currency_code is required")?;
        let _callback_status = required_trimmed(payload.status, "status is required")?;
        let amount_minor_units = payload.amount_minor_units.ok_or_else(|| {
            HappyRouteError::BadRequest("amount_minor_units is required".to_owned())
        })?;

        Ok(Self {
            provider_submission_id: payment_id,
            payer_pi_uid,
            amount_minor_units,
            currency_code,
        })
    }
}

impl PaymentCallbackPayload {
    fn raw_fields(&self) -> RawPaymentCallbackFields {
        RawPaymentCallbackFields {
            provider_submission_id: trimmed_optional(self.payment_id.as_deref()),
            payer_pi_uid: trimmed_optional(self.payer_pi_uid.as_deref()),
            amount_minor_units: self.amount_minor_units,
            currency_code: trimmed_optional(self.currency_code.as_deref()),
            txid: trimmed_optional(self.txid.as_deref()),
            callback_status: trimmed_optional(self.status.as_deref()),
        }
    }
}

fn parse_callback_payload(
    raw_body_bytes: &[u8],
) -> Result<PaymentCallbackPayload, HappyRouteError> {
    serde_json::from_slice(raw_body_bytes)
        .map_err(|_| HappyRouteError::BadRequest("callback payload must be valid json".to_owned()))
}

fn required_trimmed(value: Option<String>, error: &str) -> Result<String, HappyRouteError> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HappyRouteError::BadRequest(error.to_owned()))
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub async fn get_settlement_view(
    state: &SharedState,
    settlement_case_id: &str,
    viewer_account_id: &str,
) -> Result<SettlementViewSnapshot, HappyRouteError> {
    state
        .happy_route
        .get_settlement_view(settlement_case_id, viewer_account_id)
        .await
}
