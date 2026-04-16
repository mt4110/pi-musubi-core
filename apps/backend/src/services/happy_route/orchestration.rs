use crate::SharedState;

use super::{
    callback::process_provider_callback,
    open_hold::process_open_hold_intent,
    state::OutboxCommand,
    types::{DrainOutboxOutcome, HappyRouteError},
};

pub async fn drain_outbox(state: &SharedState) -> Result<DrainOutboxOutcome, HappyRouteError> {
    let mut processed_messages = Vec::new();
    state.happy_route.prune_processed_command_inbox().await?;

    loop {
        let next_message = state.happy_route.claim_pending_outbox_message().await?;

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
                state
                    .happy_route
                    .process_refresh_promise_view(&message, &promise_intent_id)
                    .await
            }
            OutboxCommand::RefreshSettlementView { settlement_case_id } => {
                state
                    .happy_route
                    .process_refresh_settlement_view(&message, &settlement_case_id)
                    .await
            }
        };
        let processed = match processed {
            Ok(processed) => processed,
            Err(error) => {
                state
                    .happy_route
                    .record_outbox_failure(&message, &error)
                    .await?;
                return Err(error);
            }
        };

        processed_messages.push(processed);
    }

    Ok(DrainOutboxOutcome { processed_messages })
}
