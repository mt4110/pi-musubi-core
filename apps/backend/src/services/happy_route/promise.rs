use crate::SharedState;

use super::types::{HappyRouteError, PromiseIntentInput, PromiseIntentOutcome};

pub async fn create_promise_intent(
    state: &SharedState,
    initiator_account_id: &str,
    input: PromiseIntentInput,
) -> Result<PromiseIntentOutcome, HappyRouteError> {
    state
        .happy_route
        .create_promise_intent(initiator_account_id, input)
        .await
}
