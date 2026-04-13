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
    constants::{EVENT_INGEST_PROVIDER_CALLBACK, PROVIDER_CALLBACK_CONSUMER},
    outbox::insert_outbox_message,
    repository::HappyRouteWriteRepository,
    state::{OutboxCommand, OutboxMessageRecord},
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

    let (duplicate_callback, outbox_event_id) =
        {
            let mut store = state.happy_route.write().await;
            let duplicate_callback = HappyRouteWriteRepository::new(&mut store)
                .record_raw_callback(&input, raw_fields.as_ref(), &raw_callback_id, received_at);
            let outbox_event_id = insert_outbox_message(
                &mut store,
                "provider_callback",
                &raw_callback_id,
                EVENT_INGEST_PROVIDER_CALLBACK,
                OutboxCommand::IngestProviderCallback {
                    raw_callback_id: raw_callback_id.clone(),
                },
            );
            (duplicate_callback, outbox_event_id)
        };

    Ok(PaymentCallbackAccepted {
        raw_callback_id,
        duplicate_callback,
        outbox_event_ids: vec![outbox_event_id],
    })
}

pub(super) async fn process_provider_callback(
    state: &SharedState,
    message: OutboxMessageRecord,
    raw_callback_id: String,
) -> Result<ProcessedOutboxMessage, HappyRouteError> {
    let raw_callback = {
        let store = state.happy_route.read().await;
        store
            .raw_provider_callbacks_by_id
            .get(&raw_callback_id)
            .cloned()
            .ok_or_else(|| HappyRouteError::Provider {
                class: super::types::ProviderErrorClass::ManualReview,
                message: "provider callback raw evidence is missing".to_owned(),
            })?
    };

    let parsed_payload =
        parse_callback_payload(&raw_callback.raw_body_bytes).map_err(callback_review_error)?;
    let parsed =
        ParsedPaymentCallback::try_from_payload(parsed_payload).map_err(callback_review_error)?;
    let observed_amount = canonical_pi_money(parsed.amount_minor_units, &parsed.currency_code)
        .map_err(callback_review_error)?;
    let duplicate_callback = raw_callback.replay_of_raw_callback_id.is_some();

    {
        let mut store = state.happy_route.write().await;
        HappyRouteWriteRepository::new(&mut store)
            .attach_callback_amount(&raw_callback_id, &observed_amount);
    }

    let callback_context = {
        let mut store = state.happy_route.write().await;
        HappyRouteWriteRepository::new(&mut store)
            .load_callback_context(
                &parsed,
                &observed_amount,
                &raw_callback_id,
                duplicate_callback,
            )
            .map_err(callback_review_error)?
    };

    if callback_context.duplicate_callback {
        let mut store = state.happy_route.write().await;
        if let Some(outcome) = HappyRouteWriteRepository::new(&mut store)
            .payment_callback_replay_outcome(&callback_context)?
        {
            super::outbox::mark_outbox_published(&mut store, &message.event_id);
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

    let mut store = state.happy_route.write().await;
    let outcome = HappyRouteWriteRepository::new(&mut store).persist_payment_callback_result(
        &callback_context,
        observed_amount,
        verification,
        normalized_observations,
    )?;
    super::outbox::mark_outbox_published(&mut store, &message.event_id);
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
    let store = state.happy_route.read().await;
    let settlement_case = store
        .settlement_cases_by_id
        .get(settlement_case_id)
        .ok_or_else(|| {
            HappyRouteError::NotFound(
                "settlement projection has not been built for that settlement_case_id".to_owned(),
            )
        })?;
    let promise_intent = store
        .promise_intents_by_id
        .get(&settlement_case.promise_intent_id)
        .ok_or_else(|| {
            HappyRouteError::Internal("settlement case points to missing promise intent".to_owned())
        })?;
    if viewer_account_id != promise_intent.initiator_account_id
        && viewer_account_id != promise_intent.counterparty_account_id
    {
        return Err(HappyRouteError::NotFound(
            "settlement projection has not been built for that settlement_case_id".to_owned(),
        ));
    }
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

#[cfg(test)]
mod tests {
    use super::super::{
        constants::{
            EVENT_INGEST_PROVIDER_CALLBACK, EVENT_OPEN_HOLD_INTENT, OUTBOX_MANUAL_REVIEW,
            OUTBOX_PENDING, PROVIDER_CALLBACK_MAPPING_DEFER_ATTEMPTS, SETTLEMENT_CASE_FUNDED,
        },
        outbox::claim_pending_outbox_message,
        repository::HappyRouteWriteRepository,
        types::{AuthenticationInput, OpenHoldIntentPrepareOutcome, PromiseIntentInput},
    };
    use super::*;

    #[tokio::test]
    async fn malformed_callback_payload_is_accepted_as_raw_evidence() {
        let state = crate::new_state();

        let accepted = accept_payment_callback(
            &state,
            PaymentCallbackInput {
                raw_body_bytes: b"{not-json".to_vec(),
                redacted_headers: vec![("authorization".to_owned(), "[redacted]".to_owned())],
            },
        )
        .await
        .expect("malformed callback should still be accepted as raw evidence");
        assert_eq!(accepted.outbox_event_ids.len(), 1);

        let store = state.happy_route.read().await;
        assert_eq!(store.raw_provider_callbacks_by_id.len(), 1);
        let raw_callback = store
            .raw_provider_callbacks_by_id
            .values()
            .next()
            .expect("raw callback evidence must be present");
        assert_eq!(raw_callback.raw_body, "{not-json");
        assert_eq!(raw_callback.raw_body_bytes, b"{not-json".to_vec());
        assert_eq!(raw_callback.provider_submission_id, None);
        assert_eq!(raw_callback.redacted_headers.len(), 1);
    }

    #[tokio::test]
    async fn unmapped_callback_is_deferred_before_provider_callback_processing_review() {
        let state = crate::new_state();

        let accepted = accept_payment_callback(
            &state,
            PaymentCallbackInput {
                raw_body_bytes: serde_json::json!({
                    "payment_id": "unknown-payment",
                    "payer_pi_uid": "pi-user-a",
                    "amount_minor_units": 10000,
                    "currency_code": "PI",
                    "txid": "pi-tx-unmapped",
                    "status": "completed"
                })
                .to_string()
                .into_bytes(),
                redacted_headers: vec![("x-provider-event".to_owned(), "evt-1".to_owned())],
            },
        )
        .await
        .expect("unmapped callback should be accepted as raw evidence");
        assert_eq!(accepted.outbox_event_ids.len(), 1);

        let error = crate::services::happy_route::drain_outbox(&state)
            .await
            .expect_err("unmapped callback processing should defer first");

        match error {
            HappyRouteError::ProviderCallbackMappingDeferred(message) => {
                assert!(message.contains("provider submission mapping is not ready"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        force_provider_callback_retry_available(&state).await;
        force_provider_callback_attempt_count(&state, PROVIDER_CALLBACK_MAPPING_DEFER_ATTEMPTS - 1)
            .await;

        let error = crate::services::happy_route::drain_outbox(&state)
            .await
            .expect_err("exhausted unmapped callback should require review");
        assert!(matches!(
            error,
            HappyRouteError::ProviderCallbackMappingDeferred(_)
        ));

        let store = state.happy_route.read().await;
        assert_eq!(store.raw_provider_callbacks_by_id.len(), 1);
        let raw_callback = store
            .raw_provider_callbacks_by_id
            .values()
            .next()
            .expect("raw callback evidence must be present");
        assert_eq!(
            raw_callback.provider_submission_id.as_deref(),
            Some("unknown-payment")
        );
        assert_eq!(raw_callback.callback_status.as_deref(), Some("completed"));
        assert!(raw_callback.amount.is_some());
        assert_eq!(raw_callback.signature_valid, None);
        let callback_message = store
            .outbox_messages_by_id
            .values()
            .find(|message| message.event_type == EVENT_INGEST_PROVIDER_CALLBACK)
            .expect("callback outbox message must remain visible for review");
        assert_eq!(callback_message.delivery_status, OUTBOX_MANUAL_REVIEW);
        assert_eq!(
            callback_message.last_error_class.as_deref(),
            Some("manual_review")
        );
    }

    #[tokio::test]
    async fn out_of_order_callback_retries_until_submission_mapping_exists() {
        let state = crate::new_state();
        let initiator = crate::services::happy_route::authenticate_pi_account(
            &state,
            AuthenticationInput {
                pi_uid: "pi-user-out-of-order-a".to_owned(),
                username: "out-of-order-a".to_owned(),
                wallet_address: Some("wallet-out-of-order-a".to_owned()),
                access_token: "access-token-out-of-order-a".to_owned(),
            },
        )
        .await
        .expect("initiator sign-in should work");
        let counterparty = crate::services::happy_route::authenticate_pi_account(
            &state,
            AuthenticationInput {
                pi_uid: "pi-user-out-of-order-b".to_owned(),
                username: "out-of-order-b".to_owned(),
                wallet_address: Some("wallet-out-of-order-b".to_owned()),
                access_token: "access-token-out-of-order-b".to_owned(),
            },
        )
        .await
        .expect("counterparty sign-in should work");
        let promise = crate::services::happy_route::create_promise_intent(
            &state,
            &initiator.account_id,
            PromiseIntentInput {
                internal_idempotency_key: "promise-intent-out-of-order".to_owned(),
                realm_id: "realm-out-of-order".to_owned(),
                counterparty_account_id: counterparty.account_id,
                deposit_amount_minor_units: 10000,
                currency_code: "PI".to_owned(),
            },
        )
        .await
        .expect("promise intent should be created");

        let (settlement_submission_id, provider_submission_id, provider_ref) = {
            let mut store = state.happy_route.write().await;
            let open_hold_message = claim_pending_outbox_message(&mut store)
                .expect("open hold outbox message should be claimable");
            assert_eq!(open_hold_message.event_type, EVENT_OPEN_HOLD_INTENT);
            let prepare = match HappyRouteWriteRepository::new(&mut store)
                .prepare_open_hold_intent(&open_hold_message, &promise.settlement_case_id)
                .expect("open hold prepare should create pending submission")
            {
                OpenHoldIntentPrepareOutcome::Ready(prepare) => prepare,
                OpenHoldIntentPrepareOutcome::ReplayNoop(_) => {
                    panic!("first open hold prepare cannot be a replay")
                }
            };
            (
                prepare.settlement_submission_id.clone(),
                format!("pi-payment-{}", prepare.settlement_submission_id),
                format!("pi-hold-{}", prepare.settlement_case.settlement_case_id),
            )
        };

        accept_payment_callback(
            &state,
            PaymentCallbackInput {
                raw_body_bytes: serde_json::json!({
                    "payment_id": provider_submission_id,
                    "payer_pi_uid": initiator.pi_uid,
                    "amount_minor_units": 10000,
                    "currency_code": "PI",
                    "txid": "pi-tx-out-of-order",
                    "status": "completed"
                })
                .to_string()
                .into_bytes(),
                redacted_headers: vec![(
                    "x-provider-event".to_owned(),
                    "evt-out-of-order".to_owned(),
                )],
            },
        )
        .await
        .expect("out-of-order callback should be accepted as raw evidence");

        let error = crate::services::happy_route::drain_outbox(&state)
            .await
            .expect_err("callback should defer until provider submission mapping exists");
        assert!(matches!(
            error,
            HappyRouteError::ProviderCallbackMappingDeferred(_)
        ));

        {
            let store = state.happy_route.read().await;
            let callback_message = store
                .outbox_messages_by_id
                .values()
                .find(|message| message.event_type == EVENT_INGEST_PROVIDER_CALLBACK)
                .expect("callback outbox message must remain pending");
            assert_eq!(callback_message.delivery_status, OUTBOX_PENDING);
            assert_eq!(
                callback_message.last_error_class.as_deref(),
                Some("retryable")
            );
        }

        {
            let mut store = state.happy_route.write().await;
            let submission = store
                .settlement_submissions_by_id
                .get_mut(&settlement_submission_id)
                .expect("pending submission should still exist");
            submission.provider_submission_id = Some(provider_submission_id.clone());
            submission.provider_ref = Some(provider_ref);
            submission.provider_idempotency_key = "pi:out-of-order-test".to_owned();
            submission.submission_status = "accepted".to_owned();
            submission.updated_at = Utc::now();
            store
                .settlement_submission_id_by_provider_submission_id
                .insert(provider_submission_id.clone(), settlement_submission_id);
        }
        force_provider_callback_retry_available(&state).await;

        crate::services::happy_route::drain_outbox(&state)
            .await
            .expect("deferred callback should process after mapping exists");

        let store = state.happy_route.read().await;
        let settlement_case = store
            .settlement_cases_by_id
            .get(&promise.settlement_case_id)
            .expect("settlement case should exist");
        assert_eq!(settlement_case.case_status, SETTLEMENT_CASE_FUNDED);
        assert!(store.payment_receipts_by_id.values().any(|receipt| {
            receipt.external_payment_id == provider_submission_id
                && receipt.raw_callback_id == accepted_raw_callback_id(&store)
        }));
        assert_eq!(store.ledger_journal_order.len(), 1);
    }

    async fn force_provider_callback_retry_available(state: &crate::SharedState) {
        let mut store = state.happy_route.write().await;
        for message in store.outbox_messages_by_id.values_mut() {
            if message.event_type == EVENT_INGEST_PROVIDER_CALLBACK {
                message.available_at = Utc::now();
            }
        }
    }

    async fn force_provider_callback_attempt_count(state: &crate::SharedState, attempt_count: i32) {
        let mut store = state.happy_route.write().await;
        for message in store.outbox_messages_by_id.values_mut() {
            if message.event_type == EVENT_INGEST_PROVIDER_CALLBACK {
                message.attempt_count = attempt_count;
                message.available_at = Utc::now();
            }
        }
    }

    fn accepted_raw_callback_id(store: &super::super::state::HappyRouteState) -> String {
        store
            .raw_provider_callbacks_by_id
            .keys()
            .next()
            .expect("one raw callback should be present")
            .clone()
    }
}
