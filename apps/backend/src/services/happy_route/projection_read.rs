use crate::SharedState;

use super::types::{
    ExpandedSettlementViewSnapshot, HappyRouteError, ProjectionRebuildOutcome,
    PromiseProjectionSnapshot, TrustSnapshot,
};

pub async fn get_promise_projection(
    state: &SharedState,
    promise_intent_id: &str,
    viewer_account_id: &str,
) -> Result<PromiseProjectionSnapshot, HappyRouteError> {
    state
        .happy_route
        .get_promise_projection(promise_intent_id, viewer_account_id)
        .await
}

pub async fn get_expanded_settlement_view(
    state: &SharedState,
    settlement_case_id: &str,
    viewer_account_id: &str,
) -> Result<ExpandedSettlementViewSnapshot, HappyRouteError> {
    state
        .happy_route
        .get_expanded_settlement_view(settlement_case_id, viewer_account_id)
        .await
}

pub async fn get_trust_snapshot(
    state: &SharedState,
    account_id: &str,
    viewer_account_id: &str,
) -> Result<TrustSnapshot, HappyRouteError> {
    state
        .happy_route
        .get_trust_snapshot(account_id, viewer_account_id)
        .await
}

pub async fn get_realm_trust_snapshot(
    state: &SharedState,
    realm_id: &str,
    account_id: &str,
    viewer_account_id: &str,
) -> Result<TrustSnapshot, HappyRouteError> {
    state
        .happy_route
        .get_realm_trust_snapshot(realm_id, account_id, viewer_account_id)
        .await
}

pub async fn rebuild_projection_read_models(
    state: &SharedState,
) -> Result<ProjectionRebuildOutcome, HappyRouteError> {
    state.happy_route.rebuild_projection_read_models().await
}
