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
        let Some(message) = store.outbox_messages_by_id.get_mut(event_id) else {
            continue;
        };
        if message.delivery_status == OUTBOX_PENDING && message.available_at <= now {
            message.delivery_status = OUTBOX_PROCESSING.to_owned();
            message.attempt_count += 1;
            return Some(message.clone());
        }
    }

    None
}

pub(super) fn mark_outbox_published(store: &mut HappyRouteState, event_id: &str) {
    if let Some(mut message) = store.outbox_messages_by_id.remove(event_id) {
        message.delivery_status = OUTBOX_PUBLISHED.to_owned();
        message.published_at = Some(Utc::now());
        store
            .outbox_order
            .retain(|queued_event_id| queued_event_id != event_id);
    }
}

pub(super) fn mark_outbox_pending(store: &mut HappyRouteState, event_id: &str) {
    if let Some(message) = store.outbox_messages_by_id.get_mut(event_id) {
        message.delivery_status = OUTBOX_PENDING.to_owned();
        message.available_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::{claim_pending_outbox_message, insert_outbox_message, mark_outbox_published};
    use crate::services::happy_route::state::{HappyRouteState, OutboxCommand};

    #[test]
    fn published_messages_are_pruned_from_outbox_scan_order() {
        let mut store = HappyRouteState::default();
        let first_event_id = insert_outbox_message(
            &mut store,
            "settlement_case",
            "case-1",
            "OPEN_HOLD_INTENT",
            OutboxCommand::OpenHoldIntent {
                settlement_case_id: "case-1".to_owned(),
            },
        );
        let second_event_id = insert_outbox_message(
            &mut store,
            "settlement_case",
            "case-2",
            "OPEN_HOLD_INTENT",
            OutboxCommand::OpenHoldIntent {
                settlement_case_id: "case-2".to_owned(),
            },
        );

        mark_outbox_published(&mut store, &first_event_id);

        assert!(!store.outbox_messages_by_id.contains_key(&first_event_id));
        assert_eq!(store.outbox_order, vec![second_event_id.clone()]);

        let claimed = claim_pending_outbox_message(&mut store)
            .expect("remaining pending message must still be claimable");
        assert_eq!(claimed.event_id, second_event_id);
        assert_eq!(claimed.delivery_status, "processing");
    }
}
