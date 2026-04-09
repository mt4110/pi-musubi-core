use crate::SharedState;

use super::{
    inbox::prune_processed_command_inbox,
    open_hold::process_open_hold_intent,
    outbox::{claim_pending_outbox_message, mark_outbox_pending},
    projection::{process_refresh_promise_view, process_refresh_settlement_view},
    state::OutboxCommand,
    types::{DrainOutboxOutcome, HappyRouteError},
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
                mark_outbox_pending(&mut store, &message.event_id);
                return Err(error);
            }
        };

        processed_messages.push(processed);
    }

    Ok(DrainOutboxOutcome { processed_messages })
}
