use musubi_settlement_domain::{
    InternalIdempotencyKey, ProviderPayload, ProviderPayloadField, ProviderPayloadSchema,
    ProviderPayloadValue, SettlementBackend, SettlementCapability, SubmitActionCmd,
};

use crate::SharedState;

use super::{
    backend::StubPiSettlementBackend,
    common::map_backend_error,
    constants::SETTLEMENT_ORCHESTRATOR,
    repository::HappyRouteWriteRepository,
    state::OutboxMessageRecord,
    types::{
        HappyRouteError, OpenHoldIntentPrepareOutcome, ProcessedOutboxMessage,
        processed_outbox_message,
    },
};

pub(super) async fn process_open_hold_intent(
    state: &SharedState,
    message: OutboxMessageRecord,
    settlement_case_id: String,
) -> Result<ProcessedOutboxMessage, HappyRouteError> {
    let prepare = {
        let mut store = state.happy_route.write().await;
        match HappyRouteWriteRepository::new(&mut store)
            .prepare_open_hold_intent(&message, &settlement_case_id)?
        {
            OpenHoldIntentPrepareOutcome::ReplayNoop(processed_message) => {
                return Ok(processed_message);
            }
            OpenHoldIntentPrepareOutcome::Ready(prepare) => prepare,
        }
    };

    let backend = StubPiSettlementBackend::new(state.clone());
    let submission_result = backend
        .submit_action(SubmitActionCmd {
            backend: prepare.settlement_case.backend_pin.clone(),
            case_id: musubi_settlement_domain::SettlementCaseId::new(
                prepare.settlement_case.settlement_case_id.clone(),
            ),
            intent_id: musubi_settlement_domain::SettlementIntentId::new(
                prepare.settlement_intent_id.clone(),
            ),
            submission_id: musubi_settlement_domain::SettlementSubmissionId::new(
                prepare.settlement_submission_id.clone(),
            ),
            internal_idempotency_key: InternalIdempotencyKey::new(
                prepare.internal_idempotency_key.clone(),
            ),
            capability: SettlementCapability::HoldValue,
            amount: Some(prepare.promise_intent.deposit_amount.clone()),
            provider_payload: ProviderPayload::new(
                ProviderPayloadSchema::new("pi-hold-intent", 1),
                vec![
                    ProviderPayloadField::new(
                        "promise_intent_id",
                        ProviderPayloadValue::Text(
                            prepare.promise_intent.promise_intent_id.clone(),
                        ),
                    ),
                    ProviderPayloadField::new(
                        "settlement_case_id",
                        ProviderPayloadValue::Text(
                            prepare.settlement_case.settlement_case_id.clone(),
                        ),
                    ),
                    ProviderPayloadField::new(
                        "realm_id",
                        ProviderPayloadValue::Text(prepare.promise_intent.realm_id.clone()),
                    ),
                ],
            ),
        })
        .await
        .map_err(map_backend_error)?;

    let persist_result = {
        let mut store = state.happy_route.write().await;
        HappyRouteWriteRepository::new(&mut store).persist_open_hold_intent_result(
            &message,
            &prepare,
            submission_result,
        )?
    };

    Ok(processed_outbox_message(
        &message,
        SETTLEMENT_ORCHESTRATOR,
        persist_result.provider_submission_id,
        false,
    ))
}
