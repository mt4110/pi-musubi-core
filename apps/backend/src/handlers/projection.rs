use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{
    SharedState,
    handlers::{ApiResult, map_happy_route_error},
    services::happy_route::get_settlement_view as get_settlement_view_service,
};

#[derive(Debug, Serialize)]
pub struct SettlementViewResponse {
    pub settlement_case_id: String,
    pub promise_intent_id: String,
    pub realm_id: String,
    pub current_settlement_status: String,
    pub total_funded_minor_units: i128,
    pub currency_code: String,
    pub latest_journal_entry_id: Option<String>,
}

pub async fn get_settlement_view(
    State(state): State<SharedState>,
    Path(settlement_case_id): Path<String>,
) -> ApiResult<SettlementViewResponse> {
    let snapshot = get_settlement_view_service(&state, settlement_case_id.trim())
        .await
        .map_err(map_happy_route_error)?;

    Ok(Json(SettlementViewResponse {
        settlement_case_id: snapshot.settlement_case_id,
        promise_intent_id: snapshot.promise_intent_id,
        realm_id: snapshot.realm_id,
        current_settlement_status: snapshot.current_settlement_status,
        total_funded_minor_units: snapshot.total_funded_minor_units,
        currency_code: snapshot.currency_code,
        latest_journal_entry_id: snapshot.latest_journal_entry_id,
    }))
}
