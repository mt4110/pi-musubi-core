use chrono::Utc;
use std::cmp::Ordering;
use uuid::Uuid;

use crate::SharedState;

use super::{
    backend::pi_backend_descriptor,
    common::canonical_pi_money,
    constants::{
        EVENT_OPEN_HOLD_INTENT, EVENT_REFRESH_PROMISE_VIEW, PROMISE_INTENT_PROPOSED,
        SETTLEMENT_CASE_PENDING_FUNDING,
    },
    outbox::insert_outbox_message,
    state::{OutboxCommand, PromiseIntentRecord, SettlementCaseRecord},
    types::{HappyRouteError, PromiseIntentInput, PromiseIntentOutcome},
};

pub async fn create_promise_intent(
    state: &SharedState,
    initiator_account_id: &str,
    input: PromiseIntentInput,
) -> Result<PromiseIntentOutcome, HappyRouteError> {
    let deposit_amount =
        canonical_pi_money(input.deposit_amount_minor_units, &input.currency_code)?;
    let now = Utc::now();
    let mut store = state.happy_route.write().await;

    if initiator_account_id == input.counterparty_account_id {
        return Err(HappyRouteError::BadRequest(
            "initiator_account_id and counterparty_account_id must differ".to_owned(),
        ));
    }

    if !store.accounts_by_id.contains_key(initiator_account_id) {
        return Err(HappyRouteError::NotFound(
            "initiator account was not found".to_owned(),
        ));
    }

    if !store
        .accounts_by_id
        .contains_key(&input.counterparty_account_id)
    {
        return Err(HappyRouteError::NotFound(
            "counterparty account was not found".to_owned(),
        ));
    }

    let idempotency_scope = (
        initiator_account_id.to_owned(),
        input.internal_idempotency_key.clone(),
    );
    if let Some(existing_promise_intent_id) = store
        .promise_intent_id_by_internal_idempotency_key
        .get(&idempotency_scope)
        .cloned()
    {
        let existing_promise = store
            .promise_intents_by_id
            .get(&existing_promise_intent_id)
            .ok_or_else(|| {
                HappyRouteError::Internal(
                    "promise intent idempotency key points to missing promise".to_owned(),
                )
            })?;
        let existing_settlement_case_id = store
            .settlement_case_id_by_promise_intent_id
            .get(&existing_promise_intent_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::Internal(
                    "promise intent is missing its settlement case".to_owned(),
                )
            })?;
        let existing_case = store
            .settlement_cases_by_id
            .get(&existing_settlement_case_id)
            .ok_or_else(|| {
                HappyRouteError::Internal("settlement case is missing from state".to_owned())
            })?;
        let same_payload = existing_promise.initiator_account_id == initiator_account_id
            && existing_promise.realm_id == input.realm_id
            && existing_promise.counterparty_account_id == input.counterparty_account_id
            && existing_promise
                .deposit_amount
                .checked_cmp(&deposit_amount)
                .map_err(|_| {
                    HappyRouteError::BadRequest(
                        "internal_idempotency_key payload comparison failed".to_owned(),
                    )
                })?
                == Ordering::Equal;
        if !same_payload {
            return Err(HappyRouteError::BadRequest(
                "internal_idempotency_key was already used with a different Promise payload"
                    .to_owned(),
            ));
        }

        return Ok(PromiseIntentOutcome {
            promise_intent_id: existing_promise.promise_intent_id.clone(),
            settlement_case_id: existing_case.settlement_case_id.clone(),
            case_status: existing_case.case_status.clone(),
            outbox_event_ids: Vec::new(),
            replayed_intent: true,
        });
    }

    let promise_intent_id = Uuid::new_v4().to_string();
    let settlement_case_id = Uuid::new_v4().to_string();

    let promise_intent = PromiseIntentRecord {
        promise_intent_id: promise_intent_id.clone(),
        internal_idempotency_key: input.internal_idempotency_key.clone(),
        realm_id: input.realm_id.clone(),
        initiator_account_id: initiator_account_id.to_owned(),
        counterparty_account_id: input.counterparty_account_id,
        deposit_amount,
        intent_status: PROMISE_INTENT_PROPOSED.to_owned(),
        created_at: now,
        updated_at: now,
    };

    let settlement_case = SettlementCaseRecord {
        settlement_case_id: settlement_case_id.clone(),
        promise_intent_id: promise_intent_id.clone(),
        realm_id: input.realm_id,
        case_status: SETTLEMENT_CASE_PENDING_FUNDING.to_owned(),
        backend_pin: pi_backend_descriptor().pin(),
        created_at: now,
        updated_at: now,
    };

    store
        .promise_intent_id_by_internal_idempotency_key
        .insert(idempotency_scope, promise_intent_id.clone());
    store
        .settlement_case_id_by_promise_intent_id
        .insert(promise_intent_id.clone(), settlement_case_id.clone());
    store
        .promise_intents_by_id
        .insert(promise_intent_id.clone(), promise_intent);
    store
        .settlement_cases_by_id
        .insert(settlement_case_id.clone(), settlement_case);

    let hold_event_id = insert_outbox_message(
        &mut store,
        "settlement_case",
        &settlement_case_id,
        EVENT_OPEN_HOLD_INTENT,
        OutboxCommand::OpenHoldIntent {
            settlement_case_id: settlement_case_id.clone(),
        },
    );
    let promise_view_event_id = insert_outbox_message(
        &mut store,
        "promise_intent",
        &promise_intent_id,
        EVENT_REFRESH_PROMISE_VIEW,
        OutboxCommand::RefreshPromiseView {
            promise_intent_id: promise_intent_id.clone(),
        },
    );

    Ok(PromiseIntentOutcome {
        promise_intent_id,
        settlement_case_id,
        case_status: SETTLEMENT_CASE_PENDING_FUNDING.to_owned(),
        outbox_event_ids: vec![hold_event_id, promise_view_event_id],
        replayed_intent: false,
    })
}
