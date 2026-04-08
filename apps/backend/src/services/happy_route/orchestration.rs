use crate::SharedState;

use super::{
    open_hold::process_open_hold_intent,
    outbox::claim_pending_outbox_message,
    projection::{process_refresh_promise_view, process_refresh_settlement_view},
    state::OutboxCommand,
    types::{DrainOutboxOutcome, HappyRouteError},
};

pub async fn drain_outbox(state: &SharedState) -> Result<DrainOutboxOutcome, HappyRouteError> {
    let mut processed_messages = Vec::new();

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
                process_open_hold_intent(state, message, settlement_case_id).await?
            }
            OutboxCommand::RefreshPromiseView { promise_intent_id } => {
                process_refresh_promise_view(state, message, promise_intent_id).await?
            }
            OutboxCommand::RefreshSettlementView { settlement_case_id } => {
                process_refresh_settlement_view(state, message, settlement_case_id).await?
            }
        };

        processed_messages.push(processed);
    }

    Ok(DrainOutboxOutcome { processed_messages })
}
