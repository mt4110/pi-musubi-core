use crate::SharedState;

use super::{
    callback::process_provider_callback,
    inbox::prune_processed_command_inbox,
    open_hold::process_open_hold_intent,
    outbox::{
        claim_pending_outbox_message, mark_outbox_manual_review, mark_outbox_quarantined,
        mark_outbox_retry_pending,
    },
    projection::{process_refresh_promise_view, process_refresh_settlement_view},
    state::{HappyRouteState, OutboxCommand, OutboxMessageRecord},
    types::{DrainOutboxOutcome, HappyRouteError, ProviderErrorClass},
};

pub async fn drain_outbox(state: &SharedState) -> Result<DrainOutboxOutcome, HappyRouteError> {
    let mut processed_messages = Vec::new();
    {
        let mut store = state.happy_route.write().await;
        prune_processed_command_inbox(&mut store);
    }

    loop {
        let next_message = {
            let mut store = state.happy_route.write().await;
            claim_pending_outbox_message(&mut store)
        };

        let Some(message) = next_message else {
            break;
        };

        let processed = match message.command.clone() {
            OutboxCommand::OpenHoldIntent { settlement_case_id } => {
                process_open_hold_intent(state, message.clone(), settlement_case_id).await
            }
            OutboxCommand::IngestProviderCallback { raw_callback_id } => {
                process_provider_callback(state, message.clone(), raw_callback_id).await
            }
            OutboxCommand::RefreshPromiseView { promise_intent_id } => {
                process_refresh_promise_view(state, message.clone(), promise_intent_id).await
            }
            OutboxCommand::RefreshSettlementView { settlement_case_id } => {
                process_refresh_settlement_view(state, message.clone(), settlement_case_id).await
            }
        };
        let processed = match processed {
            Ok(processed) => processed,
            Err(error) => {
                let mut store = state.happy_route.write().await;
                record_outbox_failure(&mut store, &message, &error);
                return Err(error);
            }
        };

        processed_messages.push(processed);
    }

    Ok(DrainOutboxOutcome { processed_messages })
}

fn record_outbox_failure(
    store: &mut HappyRouteState,
    message: &OutboxMessageRecord,
    error: &HappyRouteError,
) {
    match error.provider_error_class() {
        Some(ProviderErrorClass::Retryable) | None => {
            if provider_callback_mapping_retry_window_exhausted(message, error) {
                mark_outbox_manual_review(
                    store,
                    &message.event_id,
                    "manual_review",
                    error.message(),
                );
            } else {
                mark_outbox_retry_pending(store, &message.event_id, "retryable", error.message());
            }
        }
        Some(ProviderErrorClass::ManualReview) => {
            mark_outbox_manual_review(store, &message.event_id, "manual_review", error.message());
        }
        Some(ProviderErrorClass::Terminal) => {
            mark_outbox_quarantined(store, &message.event_id, "terminal", error.message());
        }
    }
}

fn provider_callback_mapping_retry_window_exhausted(
    message: &OutboxMessageRecord,
    error: &HappyRouteError,
) -> bool {
    matches!(
        message.command,
        OutboxCommand::IngestProviderCallback { .. }
    ) && error.is_provider_callback_mapping_deferred()
        && message.attempt_count >= super::constants::PROVIDER_CALLBACK_MAPPING_DEFER_ATTEMPTS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::happy_route::{
        constants::{
            EVENT_INGEST_PROVIDER_CALLBACK, OUTBOX_MANUAL_REVIEW, OUTBOX_PENDING,
            OUTBOX_QUARANTINED, PROVIDER_CALLBACK_MAPPING_DEFER_ATTEMPTS,
        },
        outbox::{claim_pending_outbox_message, insert_outbox_message},
        state::OutboxCommand,
    };

    #[test]
    fn terminal_provider_errors_quarantine_claimed_outbox_messages() {
        let mut store = HappyRouteState::default();
        let event_id = insert_open_hold_message(&mut store);
        claim_pending_outbox_message(&mut store).expect("message must be claimable");

        record_outbox_failure(
            &mut store,
            &claimed_message(&event_id),
            &HappyRouteError::Provider {
                class: ProviderErrorClass::Terminal,
                message: "terminal provider failure".to_owned(),
            },
        );

        let message = store
            .outbox_messages_by_id
            .get(&event_id)
            .expect("quarantined message must stay visible");
        assert_eq!(message.delivery_status, OUTBOX_QUARANTINED);
        assert_eq!(message.last_error_class.as_deref(), Some("terminal"));
    }

    #[test]
    fn manual_review_provider_errors_do_not_retry_claimed_outbox_messages() {
        let mut store = HappyRouteState::default();
        let event_id = insert_open_hold_message(&mut store);
        claim_pending_outbox_message(&mut store).expect("message must be claimable");

        record_outbox_failure(
            &mut store,
            &claimed_message(&event_id),
            &HappyRouteError::Provider {
                class: ProviderErrorClass::ManualReview,
                message: "mapping invalid".to_owned(),
            },
        );

        let message = store
            .outbox_messages_by_id
            .get(&event_id)
            .expect("manual review message must stay visible");
        assert_eq!(message.delivery_status, OUTBOX_MANUAL_REVIEW);
        assert!(claim_pending_outbox_message(&mut store).is_none());
    }

    #[test]
    fn retryable_provider_errors_return_claimed_outbox_messages_to_pending() {
        let mut store = HappyRouteState::default();
        let event_id = insert_open_hold_message(&mut store);
        claim_pending_outbox_message(&mut store).expect("message must be claimable");

        record_outbox_failure(
            &mut store,
            &claimed_message(&event_id),
            &HappyRouteError::Provider {
                class: ProviderErrorClass::Retryable,
                message: "timeout".to_owned(),
            },
        );

        let message = store
            .outbox_messages_by_id
            .get(&event_id)
            .expect("retryable message must stay visible");
        assert_eq!(message.delivery_status, OUTBOX_PENDING);
        assert_eq!(message.last_error_class.as_deref(), Some("retryable"));
    }

    #[test]
    fn deferred_provider_callback_mapping_retries_before_manual_review() {
        let mut store = HappyRouteState::default();
        let event_id = insert_provider_callback_message(&mut store);
        let first_claim = claim_pending_outbox_message(&mut store)
            .expect("provider callback message must be claimable");

        record_outbox_failure(
            &mut store,
            &first_claim,
            &HappyRouteError::ProviderCallbackMappingDeferred(
                "provider submission mapping is not ready".to_owned(),
            ),
        );

        let message = store
            .outbox_messages_by_id
            .get(&event_id)
            .expect("deferred message must stay visible");
        assert_eq!(message.delivery_status, OUTBOX_PENDING);
        assert_eq!(message.last_error_class.as_deref(), Some("retryable"));

        let exhausted_claim = OutboxMessageRecord {
            attempt_count: PROVIDER_CALLBACK_MAPPING_DEFER_ATTEMPTS,
            ..first_claim
        };
        record_outbox_failure(
            &mut store,
            &exhausted_claim,
            &HappyRouteError::ProviderCallbackMappingDeferred(
                "provider submission mapping is not ready".to_owned(),
            ),
        );

        let message = store
            .outbox_messages_by_id
            .get(&event_id)
            .expect("manual-review message must stay visible");
        assert_eq!(message.delivery_status, OUTBOX_MANUAL_REVIEW);
        assert_eq!(message.last_error_class.as_deref(), Some("manual_review"));
    }

    fn insert_open_hold_message(store: &mut HappyRouteState) -> String {
        insert_outbox_message(
            store,
            "settlement_case",
            "case-provider-error",
            "OPEN_HOLD_INTENT",
            OutboxCommand::OpenHoldIntent {
                settlement_case_id: "case-provider-error".to_owned(),
            },
        )
    }

    fn insert_provider_callback_message(store: &mut HappyRouteState) -> String {
        insert_outbox_message(
            store,
            "provider_callback",
            "raw-callback-deferred",
            EVENT_INGEST_PROVIDER_CALLBACK,
            OutboxCommand::IngestProviderCallback {
                raw_callback_id: "raw-callback-deferred".to_owned(),
            },
        )
    }

    fn claimed_message(event_id: &str) -> OutboxMessageRecord {
        OutboxMessageRecord {
            event_id: event_id.to_owned(),
            idempotency_key: "claimed-idempotency-key".to_owned(),
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id: "case-provider-error".to_owned(),
            event_type: "OPEN_HOLD_INTENT".to_owned(),
            schema_version: 1,
            command: OutboxCommand::OpenHoldIntent {
                settlement_case_id: "case-provider-error".to_owned(),
            },
            delivery_status: "processing".to_owned(),
            attempt_count: 1,
            last_error_class: None,
            last_error_message: None,
            available_at: chrono::Utc::now(),
            published_at: None,
            created_at: chrono::Utc::now(),
        }
    }
}
