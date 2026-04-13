use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{
    constants::{
        OUTBOX_MANUAL_REVIEW, OUTBOX_PENDING, OUTBOX_PROCESSING, OUTBOX_PUBLISHED,
        OUTBOX_QUARANTINED, OUTBOX_RETRY_BACKOFF_MILLIS,
    },
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
        last_error_class: None,
        last_error_message: None,
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

pub(super) fn mark_outbox_retry_pending(
    store: &mut HappyRouteState,
    event_id: &str,
    error_class: &str,
    error_message: &str,
) {
    if let Some(message) = store.outbox_messages_by_id.get_mut(event_id) {
        message.delivery_status = OUTBOX_PENDING.to_owned();
        message.last_error_class = Some(error_class.to_owned());
        message.last_error_message = Some(error_message.to_owned());
        message.available_at = Utc::now() + Duration::milliseconds(OUTBOX_RETRY_BACKOFF_MILLIS);
    }
}

pub(super) fn mark_outbox_manual_review(
    store: &mut HappyRouteState,
    event_id: &str,
    error_class: &str,
    error_message: &str,
) {
    if let Some(message) = store.outbox_messages_by_id.get_mut(event_id) {
        message.delivery_status = OUTBOX_MANUAL_REVIEW.to_owned();
        message.last_error_class = Some(error_class.to_owned());
        message.last_error_message = Some(error_message.to_owned());
        store
            .outbox_order
            .retain(|queued_event_id| queued_event_id != event_id);
    }
}

pub(super) fn mark_outbox_quarantined(
    store: &mut HappyRouteState,
    event_id: &str,
    error_class: &str,
    error_message: &str,
) {
    if let Some(message) = store.outbox_messages_by_id.get_mut(event_id) {
        message.delivery_status = OUTBOX_QUARANTINED.to_owned();
        message.last_error_class = Some(error_class.to_owned());
        message.last_error_message = Some(error_message.to_owned());
        store
            .outbox_order
            .retain(|queued_event_id| queued_event_id != event_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        claim_pending_outbox_message, insert_outbox_message, mark_outbox_manual_review,
        mark_outbox_published, mark_outbox_quarantined, mark_outbox_retry_pending,
    };
    use crate::services::happy_route::{
        constants::{OUTBOX_MANUAL_REVIEW, OUTBOX_QUARANTINED},
        state::{HappyRouteState, OutboxCommand},
    };

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

    #[test]
    fn failed_message_is_deferred_so_later_pending_events_can_run() {
        let mut store = HappyRouteState::default();
        let first_event_id = insert_outbox_message(
            &mut store,
            "settlement_case",
            "case-failed",
            "OPEN_HOLD_INTENT",
            OutboxCommand::OpenHoldIntent {
                settlement_case_id: "case-failed".to_owned(),
            },
        );
        let second_event_id = insert_outbox_message(
            &mut store,
            "settlement_case",
            "case-later",
            "OPEN_HOLD_INTENT",
            OutboxCommand::OpenHoldIntent {
                settlement_case_id: "case-later".to_owned(),
            },
        );

        let first_claim = claim_pending_outbox_message(&mut store)
            .expect("first pending message must be claimable");
        assert_eq!(first_claim.event_id, first_event_id);

        mark_outbox_retry_pending(&mut store, &first_event_id, "retryable", "test failure");

        let second_claim = claim_pending_outbox_message(&mut store)
            .expect("later pending message must not be starved by a failed retry");
        assert_eq!(second_claim.event_id, second_event_id);
    }

    #[test]
    fn terminal_review_messages_leave_active_scan_order_but_remain_visible() {
        let mut store = HappyRouteState::default();
        let manual_review_event_id = insert_open_hold_message(&mut store, "case-manual-review");
        let quarantined_event_id = insert_open_hold_message(&mut store, "case-quarantined");
        let still_pending_event_id = insert_open_hold_message(&mut store, "case-pending");

        mark_outbox_manual_review(
            &mut store,
            &manual_review_event_id,
            "manual_review",
            "operator review required",
        );
        mark_outbox_quarantined(
            &mut store,
            &quarantined_event_id,
            "terminal",
            "terminal failure",
        );

        let manual_review_message = store
            .outbox_messages_by_id
            .get(&manual_review_event_id)
            .expect("manual-review rows remain visible for review");
        assert_eq!(manual_review_message.delivery_status, OUTBOX_MANUAL_REVIEW);
        let quarantined_message = store
            .outbox_messages_by_id
            .get(&quarantined_event_id)
            .expect("quarantined rows remain visible for review");
        assert_eq!(quarantined_message.delivery_status, OUTBOX_QUARANTINED);

        assert_eq!(store.outbox_order, vec![still_pending_event_id.clone()]);
        let claimed = claim_pending_outbox_message(&mut store)
            .expect("pending rows should remain claimable after terminal pruning");
        assert_eq!(claimed.event_id, still_pending_event_id);
    }

    fn insert_open_hold_message(store: &mut HappyRouteState, settlement_case_id: &str) -> String {
        insert_outbox_message(
            store,
            "settlement_case",
            settlement_case_id,
            "OPEN_HOLD_INTENT",
            OutboxCommand::OpenHoldIntent {
                settlement_case_id: settlement_case_id.to_owned(),
            },
        )
    }
}
