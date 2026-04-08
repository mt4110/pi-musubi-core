use chrono::Utc;
use musubi_settlement_domain::{CurrencyCode, Money};

use crate::SharedState;

use super::{
    constants::{PI_CURRENCY_CODE, PI_SCALE, PROJECTION_BUILDER},
    outbox::mark_outbox_published,
    state::{
        CommandInboxRecord, HappyRouteState, OutboxMessageRecord, PromiseViewRecord,
        SettlementViewRecord,
    },
    types::{HappyRouteError, ProcessedOutboxMessage, processed_outbox_message},
};

pub(super) async fn process_refresh_promise_view(
    state: &SharedState,
    message: OutboxMessageRecord,
    promise_intent_id: String,
) -> Result<ProcessedOutboxMessage, HappyRouteError> {
    let mut store = state.happy_route.write().await;
    let inbox_key = (PROJECTION_BUILDER.to_owned(), message.event_id.clone());

    if store.command_inbox_by_key.contains_key(&inbox_key) {
        mark_outbox_published(&mut store, &message.event_id);
        return Ok(processed_outbox_message(
            &message,
            PROJECTION_BUILDER,
            None,
            true,
        ));
    }

    let promise_intent = store
        .promise_intents_by_id
        .get(&promise_intent_id)
        .cloned()
        .ok_or_else(|| {
            HappyRouteError::NotFound(
                "promise intent referenced by projection refresh is missing".to_owned(),
            )
        })?;
    let settlement_case_id = store
        .settlement_case_id_by_promise_intent_id
        .get(&promise_intent_id)
        .cloned();

    store.command_inbox_by_key.insert(
        inbox_key,
        CommandInboxRecord {
            consumer_name: PROJECTION_BUILDER.to_owned(),
            source_message_id: message.event_id.clone(),
            received_at: Utc::now(),
            processed_at: Some(Utc::now()),
        },
    );
    store.promise_views_by_id.insert(
        promise_intent_id.clone(),
        PromiseViewRecord {
            promise_intent_id,
            realm_id: promise_intent.realm_id,
            initiator_account_id: promise_intent.initiator_account_id,
            counterparty_account_id: promise_intent.counterparty_account_id,
            current_intent_status: promise_intent.intent_status,
            latest_settlement_case_id: settlement_case_id,
            last_projected_at: Utc::now(),
        },
    );
    mark_outbox_published(&mut store, &message.event_id);

    Ok(processed_outbox_message(
        &message,
        PROJECTION_BUILDER,
        None,
        false,
    ))
}

pub(super) async fn process_refresh_settlement_view(
    state: &SharedState,
    message: OutboxMessageRecord,
    settlement_case_id: String,
) -> Result<ProcessedOutboxMessage, HappyRouteError> {
    let mut store = state.happy_route.write().await;
    let inbox_key = (PROJECTION_BUILDER.to_owned(), message.event_id.clone());

    if store.command_inbox_by_key.contains_key(&inbox_key) {
        mark_outbox_published(&mut store, &message.event_id);
        return Ok(processed_outbox_message(
            &message,
            PROJECTION_BUILDER,
            None,
            true,
        ));
    }

    let settlement_case = store
        .settlement_cases_by_id
        .get(&settlement_case_id)
        .cloned()
        .ok_or_else(|| {
            HappyRouteError::NotFound(
                "settlement case referenced by projection refresh is missing".to_owned(),
            )
        })?;

    let total_funded = calculate_total_funded(&store, &settlement_case_id)?;
    let latest_journal_entry_id = latest_journal_entry_id(&store, &settlement_case_id);

    store.command_inbox_by_key.insert(
        inbox_key,
        CommandInboxRecord {
            consumer_name: PROJECTION_BUILDER.to_owned(),
            source_message_id: message.event_id.clone(),
            received_at: Utc::now(),
            processed_at: Some(Utc::now()),
        },
    );
    store.settlement_views_by_id.insert(
        settlement_case_id.clone(),
        SettlementViewRecord {
            settlement_case_id,
            realm_id: settlement_case.realm_id,
            promise_intent_id: settlement_case.promise_intent_id,
            latest_journal_entry_id,
            current_settlement_status: settlement_case.case_status,
            total_funded,
            last_projected_at: Utc::now(),
        },
    );
    mark_outbox_published(&mut store, &message.event_id);

    Ok(processed_outbox_message(
        &message,
        PROJECTION_BUILDER,
        None,
        false,
    ))
}

fn calculate_total_funded(
    store: &HappyRouteState,
    settlement_case_id: &str,
) -> Result<Money, HappyRouteError> {
    let mut total = Money::new(
        CurrencyCode::new(PI_CURRENCY_CODE).expect("PI currency code must remain valid"),
        0,
        PI_SCALE,
    );

    for posting in &store.ledger_postings {
        let Some(journal) = store.ledger_journals_by_id.get(&posting.journal_entry_id) else {
            continue;
        };
        if journal.settlement_case_id != settlement_case_id {
            continue;
        }
        if posting.ledger_account_code != "user_secured_funds_liability" {
            continue;
        }
        if posting.direction != "credit" {
            continue;
        }

        total = total.checked_add(&posting.amount).map_err(|_| {
            HappyRouteError::Internal(
                "ledger projection encountered incompatible funded postings".to_owned(),
            )
        })?;
    }

    Ok(total)
}

fn latest_journal_entry_id(store: &HappyRouteState, settlement_case_id: &str) -> Option<String> {
    store
        .ledger_journal_order
        .iter()
        .rev()
        .find_map(|journal_entry_id| {
            store
                .ledger_journals_by_id
                .get(journal_entry_id)
                .filter(|journal| journal.settlement_case_id == settlement_case_id)
                .map(|journal| journal.journal_entry_id.clone())
        })
}
