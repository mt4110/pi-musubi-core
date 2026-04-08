use chrono::Utc;
use musubi_settlement_domain::{
    NormalizeCallbackCmd, PaymentReceiptId, ProviderCallbackId, SettlementBackend,
    VerifyReceiptCmd, VerifyReceiptExpectation,
};
use uuid::Uuid;

use crate::SharedState;

use super::{
    backend::StubPiSettlementBackend,
    common::{canonical_pi_money, map_backend_error},
    repository::HappyRouteWriteRepository,
    types::{
        HappyRouteError, PaymentCallbackInput, PaymentCallbackOutcome, SettlementViewSnapshot,
    },
};

pub async fn ingest_payment_callback(
    state: &SharedState,
    input: PaymentCallbackInput,
) -> Result<PaymentCallbackOutcome, HappyRouteError> {
    let observed_amount = canonical_pi_money(input.amount_minor_units, &input.currency_code)?;
    let raw_callback_id = Uuid::new_v4().to_string();

    let callback_context = {
        let mut store = state.happy_route.write().await;
        HappyRouteWriteRepository::new(&mut store).record_raw_callback_and_load_context(
            &input,
            &observed_amount,
            &raw_callback_id,
            Utc::now(),
        )?
    };

    let backend = StubPiSettlementBackend::new(state.clone());
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
            receipt_id: PaymentReceiptId::new(format!("receipt-{}", input.payment_id)),
            raw_callback_ref: ProviderCallbackId::new(callback_context.raw_callback_id.clone()),
            expected: Some(VerifyReceiptExpectation::Amount(
                callback_context.promise_intent.deposit_amount.clone(),
            )),
        })
        .await
        .map_err(map_backend_error)?;

    let mut store = state.happy_route.write().await;
    HappyRouteWriteRepository::new(&mut store).persist_payment_callback_result(
        &callback_context,
        &input.payment_id,
        observed_amount,
        verification,
        normalized_observations,
    )
}

pub async fn get_settlement_view(
    state: &SharedState,
    settlement_case_id: &str,
) -> Result<SettlementViewSnapshot, HappyRouteError> {
    let store = state.happy_route.read().await;
    let settlement_view = store
        .settlement_views_by_id
        .get(settlement_case_id)
        .ok_or_else(|| {
            HappyRouteError::NotFound(
                "settlement projection has not been built for that settlement_case_id".to_owned(),
            )
        })?;

    Ok(SettlementViewSnapshot {
        settlement_case_id: settlement_view.settlement_case_id.clone(),
        promise_intent_id: settlement_view.promise_intent_id.clone(),
        realm_id: settlement_view.realm_id.clone(),
        current_settlement_status: settlement_view.current_settlement_status.clone(),
        total_funded_minor_units: settlement_view.total_funded.minor_units(),
        currency_code: settlement_view.total_funded.currency().as_str().to_owned(),
        latest_journal_entry_id: settlement_view.latest_journal_entry_id.clone(),
    })
}
