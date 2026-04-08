use chrono::Utc;
use uuid::Uuid;

use super::{
    constants::{OUTBOX_PENDING, OUTBOX_PROCESSING, OUTBOX_PUBLISHED},
    state::{HappyRouteState, OutboxCommand, OutboxMessageRecord},
};

pub(super) fn insert_outbox_message(
    store: &mut HappyRouteState,
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
    command: OutboxCommand,
) -> String {
    let event_id = Uuid::new_v4().to_string();
    let record = OutboxMessageRecord {
        event_id: event_id.clone(),
        idempotency_key: Uuid::new_v4().to_string(),
        aggregate_type: aggregate_type.to_owned(),
        aggregate_id: aggregate_id.to_owned(),
        event_type: event_type.to_owned(),
        schema_version: 1,
        command,
        delivery_status: OUTBOX_PENDING.to_owned(),
        attempt_count: 0,
        available_at: Utc::now(),
        published_at: None,
        created_at: Utc::now(),
    };
    store.outbox_order.push(event_id.clone());
    store.outbox_messages_by_id.insert(event_id.clone(), record);
    event_id
}

pub(super) fn claim_pending_outbox_message(
    store: &mut HappyRouteState,
) -> Option<OutboxMessageRecord> {
    let now = Utc::now();

    for event_id in &store.outbox_order {
        let message = store.outbox_messages_by_id.get_mut(event_id)?;
        if message.delivery_status == OUTBOX_PENDING && message.available_at <= now {
            message.delivery_status = OUTBOX_PROCESSING.to_owned();
            message.attempt_count += 1;
            return Some(message.clone());
        }
    }

    None
}

pub(super) fn mark_outbox_published(store: &mut HappyRouteState, event_id: &str) {
    if let Some(message) = store.outbox_messages_by_id.get_mut(event_id) {
        message.delivery_status = OUTBOX_PUBLISHED.to_owned();
        message.published_at = Some(Utc::now());
    }
}
