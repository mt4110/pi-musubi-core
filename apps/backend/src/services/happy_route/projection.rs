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

    if let Some(command_inbox) = store.command_inbox_by_key.get(&inbox_key) {
        if command_inbox.processed_at.is_some() {
            mark_outbox_published(&mut store, &message.event_id);
            return Ok(processed_outbox_message(
                &message,
                PROJECTION_BUILDER,
                None,
                true,
            ));
        }
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

    store
        .command_inbox_by_key
        .entry(inbox_key.clone())
        .or_insert_with(|| CommandInboxRecord {
            consumer_name: PROJECTION_BUILDER.to_owned(),
            source_message_id: message.event_id.clone(),
            received_at: Utc::now(),
            processed_at: None,
        });
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
    if let Some(command_inbox) = store.command_inbox_by_key.get_mut(&inbox_key) {
        command_inbox.processed_at = Some(Utc::now());
    }
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

    if let Some(command_inbox) = store.command_inbox_by_key.get(&inbox_key) {
        if command_inbox.processed_at.is_some() {
            mark_outbox_published(&mut store, &message.event_id);
            return Ok(processed_outbox_message(
                &message,
                PROJECTION_BUILDER,
                None,
                true,
            ));
        }
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

    store
        .command_inbox_by_key
        .entry(inbox_key.clone())
        .or_insert_with(|| CommandInboxRecord {
            consumer_name: PROJECTION_BUILDER.to_owned(),
            source_message_id: message.event_id.clone(),
            received_at: Utc::now(),
            processed_at: None,
        });
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
    if let Some(command_inbox) = store.command_inbox_by_key.get_mut(&inbox_key) {
        command_inbox.processed_at = Some(Utc::now());
    }
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

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use musubi_settlement_domain::{BackendKey, BackendPin, BackendVersion, CurrencyCode, Money};

    use super::process_refresh_promise_view;
    use crate::{
        new_state,
        services::happy_route::{
            constants::PROJECTION_BUILDER,
            state::{
                CommandInboxRecord, OutboxCommand, OutboxMessageRecord, PromiseIntentRecord,
                SettlementCaseRecord,
            },
        },
    };

    #[tokio::test]
    async fn projection_retry_with_unprocessed_inbox_rebuilds_view() {
        let state = new_state();
        let message = OutboxMessageRecord {
            event_id: "event-1".to_owned(),
            idempotency_key: "idem-1".to_owned(),
            aggregate_type: "promise_intent".to_owned(),
            aggregate_id: "promise-1".to_owned(),
            event_type: "REFRESH_PROMISE_VIEW".to_owned(),
            schema_version: 1,
            command: OutboxCommand::RefreshPromiseView {
                promise_intent_id: "promise-1".to_owned(),
            },
            delivery_status: "processing".to_owned(),
            attempt_count: 1,
            available_at: Utc::now(),
            published_at: None,
            created_at: Utc::now(),
        };

        {
            let mut store = state.happy_route.write().await;
            store.promise_intents_by_id.insert(
                "promise-1".to_owned(),
                PromiseIntentRecord {
                    promise_intent_id: "promise-1".to_owned(),
                    internal_idempotency_key: "intent-1".to_owned(),
                    realm_id: "realm-1".to_owned(),
                    initiator_account_id: "account-a".to_owned(),
                    counterparty_account_id: "account-b".to_owned(),
                    deposit_amount: Money::new(
                        CurrencyCode::new("PI").expect("PI must be valid"),
                        10000,
                        2,
                    ),
                    intent_status: "pending_funding".to_owned(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                },
            );
            store.settlement_cases_by_id.insert(
                "case-1".to_owned(),
                SettlementCaseRecord {
                    settlement_case_id: "case-1".to_owned(),
                    promise_intent_id: "promise-1".to_owned(),
                    realm_id: "realm-1".to_owned(),
                    case_status: "pending_funding".to_owned(),
                    backend_pin: BackendPin::new(
                        BackendKey::new("stub_pi"),
                        BackendVersion::new("0.1.0"),
                    ),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                },
            );
            store
                .settlement_case_id_by_promise_intent_id
                .insert("promise-1".to_owned(), "case-1".to_owned());
            store.command_inbox_by_key.insert(
                (PROJECTION_BUILDER.to_owned(), message.event_id.clone()),
                CommandInboxRecord {
                    consumer_name: PROJECTION_BUILDER.to_owned(),
                    source_message_id: message.event_id.clone(),
                    received_at: Utc::now(),
                    processed_at: None,
                },
            );
            store
                .outbox_messages_by_id
                .insert(message.event_id.clone(), message.clone());
            store.outbox_order.push(message.event_id.clone());
        }

        let processed = process_refresh_promise_view(&state, message, "promise-1".to_owned())
            .await
            .expect("projection retry must succeed");
        assert!(!processed.already_processed);

        let store = state.happy_route.read().await;
        assert!(store.promise_views_by_id.contains_key("promise-1"));
        assert_eq!(store.outbox_order.len(), 0);
        assert!(
            store
                .command_inbox_by_key
                .get(&(PROJECTION_BUILDER.to_owned(), "event-1".to_owned()))
                .and_then(|record| record.processed_at)
                .is_some()
        );
    }
}
