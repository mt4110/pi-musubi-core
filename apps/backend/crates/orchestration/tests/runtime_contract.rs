use chrono::{TimeDelta, TimeZone, Utc};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

use musubi_orchestration::{
    AuthoritativeChange, CommandCompletion, CommandEnvelope, ConsumeOutcome, DeliveryOutcome,
    ExternalIdempotencyKey, InMemoryOrchestrationStore, NewOutboxMessage, OrchestrationError,
    OrchestrationRuntime, OrchestrationStore, OutboxDeliveryStatus, ProcessingFailure,
    QuarantineReason, RetentionPolicy, RetryPolicy, SchemaCompatibilityPolicy, WriterReadSource,
};

fn runtime() -> OrchestrationRuntime<InMemoryOrchestrationStore> {
    OrchestrationRuntime::new(
        InMemoryOrchestrationStore::default(),
        RetryPolicy {
            max_attempts: 3,
            base_delay: TimeDelta::seconds(30),
            max_delay: TimeDelta::minutes(5),
            max_jitter: TimeDelta::seconds(10),
        },
        RetentionPolicy {
            published_outbox_for: TimeDelta::hours(1),
            quarantined_outbox_for: TimeDelta::hours(12),
            completed_command_for: TimeDelta::hours(1),
            quarantined_command_for: TimeDelta::hours(12),
        },
        SchemaCompatibilityPolicy {
            max_supported_schema_version: 1,
            compatibility_window: TimeDelta::minutes(15),
        },
        TimeDelta::minutes(5),
    )
}

fn ts(seconds: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + seconds, 0).unwrap()
}

#[tokio::test]
async fn producer_truth_and_outbox_commit_together() {
    let mut runtime = runtime();
    let aggregate_id = Uuid::from_u128(0x10);
    let event_id = Uuid::from_u128(0x20);
    let idempotency_key = Uuid::from_u128(0x30);

    runtime
        .record_authoritative_write(
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "receipt_recorded".to_owned(),
                payload_json: json!({ "receipt_id": "receipt-1" }),
            },
            NewOutboxMessage::new(
                event_id,
                idempotency_key,
                "settlement_case:10",
                "settlement_case",
                aggregate_id,
                "settlement.receipt_recorded",
                1,
                json!({ "receipt_id": "receipt-1" }),
                ts(0),
                ts(0),
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(runtime.store().authoritative_changes().len(), 1);
    let message = runtime.store().outbox_message(event_id).unwrap();
    assert_eq!(message.delivery_status, OutboxDeliveryStatus::Pending);
    assert_eq!(message.aggregate_id, aggregate_id);
}

#[tokio::test]
async fn duplicate_consumer_delivery_is_a_no_op() {
    let mut runtime = runtime();
    let command_id = Uuid::from_u128(0x40);
    let source_event_id = Uuid::from_u128(0x41);
    let handled = Arc::new(AtomicUsize::new(0));

    let command = CommandEnvelope::new(
        command_id,
        source_event_id,
        "projection.refresh",
        1,
        json!({ "case_id": "case-1" }),
    )
    .unwrap();

    let handled_once = handled.clone();
    let first = runtime
        .consume_command("projection-builder", command.clone(), ts(0), move |_| {
            let handled_once = handled_once.clone();
            async move {
                handled_once.fetch_add(1, Ordering::SeqCst);
                Ok(CommandCompletion {
                    result_type: "projected".to_owned(),
                    result_json: json!({ "projection_id": "read-1" }),
                })
            }
        })
        .await
        .unwrap();

    let second = runtime
        .consume_command("projection-builder", command, ts(1), |_| async move {
            Ok(CommandCompletion {
                result_type: "should_not_run".to_owned(),
                result_json: json!({}),
            })
        })
        .await
        .unwrap();

    assert_eq!(first, ConsumeOutcome::Completed { command_id });
    assert_eq!(second, ConsumeOutcome::Duplicate { command_id });
    assert_eq!(handled.load(Ordering::SeqCst), 1);
}

#[test]
fn active_processing_lease_is_deferred_until_claim_expiry() {
    let mut store = InMemoryOrchestrationStore::default();
    let command_id = Uuid::from_u128(0x42);
    let first_seen_at = ts(0);
    let lease_until = ts(300);
    let command = CommandEnvelope::new(
        command_id,
        Uuid::from_u128(0x43),
        "projection.refresh",
        1,
        json!({ "case_id": "case-lease" }),
    )
    .unwrap();

    let first = store.begin_command(
        WriterReadSource::PrimaryWriter,
        "projection-builder",
        command.clone(),
        first_seen_at,
        lease_until,
    );
    let second = store.begin_command(
        WriterReadSource::PrimaryWriter,
        "projection-builder",
        command,
        ts(1),
        ts(301),
    );

    assert!(matches!(
        first,
        Ok(musubi_orchestration::CommandBeginOutcome::FirstSeen(_))
    ));
    assert!(matches!(
        second,
        Ok(musubi_orchestration::CommandBeginOutcome::Deferred(entry))
            if entry.available_at == lease_until
    ));
}

#[test]
fn conflicting_command_payload_is_rejected() {
    let mut store = InMemoryOrchestrationStore::default();
    let command_id = Uuid::from_u128(0x44);

    store
        .begin_command(
            WriterReadSource::PrimaryWriter,
            "projection-builder",
            CommandEnvelope::new(
                command_id,
                Uuid::from_u128(0x45),
                "projection.refresh",
                1,
                json!({ "case_id": "case-1" }),
            )
            .unwrap(),
            ts(0),
            ts(300),
        )
        .unwrap();

    let result = store.begin_command(
        WriterReadSource::PrimaryWriter,
        "projection-builder",
        CommandEnvelope::new(
            command_id,
            Uuid::from_u128(0x45),
            "projection.refresh",
            1,
            json!({ "case_id": "case-2" }),
        )
        .unwrap(),
        ts(1),
        ts(301),
    );

    assert_eq!(
        result,
        Err(OrchestrationError::ConflictingCommandEnvelope {
            consumer_name: "projection-builder".to_owned(),
            command_id,
        })
    );
}

#[tokio::test]
async fn transient_outbox_failure_schedules_retry() {
    let mut runtime = runtime();
    let aggregate_id = Uuid::from_u128(0x50);
    let event_id = Uuid::from_u128(0x51);

    runtime
        .record_authoritative_write(
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "submission_requested".to_owned(),
                payload_json: json!({ "intent_id": "intent-1" }),
            },
            NewOutboxMessage::new(
                event_id,
                Uuid::from_u128(0x52),
                "settlement_case:50",
                "settlement_case",
                aggregate_id,
                "settlement.submit_action",
                1,
                json!({ "intent_id": "intent-1" }),
                ts(0),
                ts(0),
            )
            .unwrap(),
        )
        .unwrap();

    let outcome = runtime
        .deliver_ready_outbox("settlement-relay", ts(0), |_| async {
            Err(ProcessingFailure::transient(
                "provider_timeout",
                "provider did not respond in time",
            ))
        })
        .await
        .unwrap();

    let message = runtime.store().outbox_message(event_id).unwrap();
    assert!(matches!(
        outcome,
        DeliveryOutcome::RetryScheduled {
            event_id: returned_event_id,
            ..
        } if returned_event_id == event_id
    ));
    assert_eq!(message.delivery_status, OutboxDeliveryStatus::Pending);
    assert_eq!(message.attempt_count, 1);
    assert_eq!(runtime.store().outbox_attempts(event_id).len(), 1);
}

#[tokio::test]
async fn handler_poison_pill_command_is_quarantined() {
    let mut runtime = runtime();
    let command_id = Uuid::from_u128(0x60);

    let outcome = runtime
        .consume_command(
            "settlement-consumer",
            CommandEnvelope::new(
                command_id,
                Uuid::from_u128(0x61),
                "settlement.apply_observation",
                1,
                json!({ "bad": "payload" }),
            )
            .unwrap(),
            ts(0),
            |_| async {
                Err(ProcessingFailure::poison_pill(
                    "unknown_schema",
                    "schema_version 99 is not supported by this worker",
                ))
            },
        )
        .await
        .unwrap();

    let entry = runtime
        .store()
        .command_inbox_entry("settlement-consumer", command_id)
        .unwrap();

    assert_eq!(
        outcome,
        ConsumeOutcome::Quarantined {
            command_id,
            reason: QuarantineReason::PoisonPill,
        }
    );
    assert_eq!(entry.quarantine_reason, Some(QuarantineReason::PoisonPill));
}

#[tokio::test]
async fn unknown_schema_is_deferred_then_quarantined_after_window() {
    let mut runtime = runtime();
    let command_id = Uuid::from_u128(0x62);
    let command = CommandEnvelope::new(
        command_id,
        Uuid::from_u128(0x63),
        "settlement.apply_observation",
        99,
        json!({ "schema": "future" }),
    )
    .unwrap();

    let first = runtime
        .consume_command(
            "settlement-consumer",
            command.clone(),
            ts(0),
            |_| async move {
                Ok(CommandCompletion {
                    result_type: "should_not_run".to_owned(),
                    result_json: json!({}),
                })
            },
        )
        .await
        .unwrap();

    assert!(matches!(
        first,
        ConsumeOutcome::RetryScheduled {
            command_id: returned_id,
            ..
        } if returned_id == command_id
    ));

    let second = runtime
        .consume_command(
            "settlement-consumer",
            command,
            ts(60 * 20),
            |_| async move {
                Ok(CommandCompletion {
                    result_type: "should_not_run".to_owned(),
                    result_json: json!({}),
                })
            },
        )
        .await
        .unwrap();

    assert_eq!(
        second,
        ConsumeOutcome::Quarantined {
            command_id,
            reason: QuarantineReason::PoisonPill,
        }
    );
}

#[tokio::test]
async fn deferred_command_ignores_attempt_budget_until_window_expires() {
    let mut runtime = runtime();
    let command_id = Uuid::from_u128(0x64);
    let command = CommandEnvelope::new(
        command_id,
        Uuid::from_u128(0x65),
        "settlement.apply_observation",
        99,
        json!({ "schema": "future" }),
    )
    .unwrap();

    for now in [ts(0), ts(90), ts(240), ts(500)] {
        let outcome = runtime
            .consume_command(
                "settlement-consumer",
                command.clone(),
                now,
                |_| async move {
                    Ok(CommandCompletion {
                        result_type: "should_not_run".to_owned(),
                        result_json: json!({}),
                    })
                },
            )
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            ConsumeOutcome::RetryScheduled {
                command_id: returned_id,
                ..
            } if returned_id == command_id
        ));
    }
}

#[tokio::test]
async fn deferred_outbox_ignores_attempt_budget_until_window_expires() {
    let mut runtime = runtime();
    let aggregate_id = Uuid::from_u128(0x66);
    let event_id = Uuid::from_u128(0x67);

    runtime
        .record_authoritative_write(
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "submission_requested".to_owned(),
                payload_json: json!({ "intent_id": "future-schema" }),
            },
            NewOutboxMessage::new(
                event_id,
                Uuid::from_u128(0x68),
                "settlement_case:66",
                "settlement_case",
                aggregate_id,
                "settlement.submit_action",
                99,
                json!({ "intent_id": "future-schema" }),
                ts(0),
                ts(0),
            )
            .unwrap(),
        )
        .unwrap();

    for now in [ts(0), ts(90), ts(240), ts(500)] {
        let outcome = runtime
            .deliver_ready_outbox("settlement-relay", now, |_| async {
                Ok(musubi_orchestration::DeliveryReceipt {
                    external_idempotency_key: ExternalIdempotencyKey::new("provider-key-future")
                        .unwrap(),
                })
            })
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            DeliveryOutcome::RetryScheduled {
                event_id: returned_id,
                ..
            } if returned_id == event_id
        ));
    }
}

#[tokio::test]
async fn pruning_archives_terminal_coordination_rows() {
    let mut runtime = runtime();
    let aggregate_id = Uuid::from_u128(0x70);
    let event_id = Uuid::from_u128(0x71);
    let command_id = Uuid::from_u128(0x72);

    runtime
        .record_authoritative_write(
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "submission_requested".to_owned(),
                payload_json: json!({ "intent_id": "intent-2" }),
            },
            NewOutboxMessage::new(
                event_id,
                Uuid::from_u128(0x73),
                "settlement_case:70",
                "settlement_case",
                aggregate_id,
                "settlement.submit_action",
                1,
                json!({ "intent_id": "intent-2" }),
                ts(0),
                ts(0),
            )
            .unwrap(),
        )
        .unwrap();

    runtime
        .deliver_ready_outbox("settlement-relay", ts(0), |_| async {
            Ok(musubi_orchestration::DeliveryReceipt {
                external_idempotency_key: ExternalIdempotencyKey::new("provider-key-1").unwrap(),
            })
        })
        .await
        .unwrap();

    runtime
        .consume_command(
            "projection-builder",
            CommandEnvelope::new(
                command_id,
                event_id,
                "projection.refresh",
                1,
                json!({ "case_id": "case-2" }),
            )
            .unwrap(),
            ts(0),
            |_| async {
                Ok(CommandCompletion {
                    result_type: "projected".to_owned(),
                    result_json: json!({ "projection_id": "read-2" }),
                })
            },
        )
        .await
        .unwrap();

    let prune_outcome = runtime.prune_coordination(ts(4_000)).unwrap();

    assert_eq!(prune_outcome.pruned_outbox_event_ids, vec![event_id]);
    assert_eq!(prune_outcome.pruned_command_keys.len(), 1);
    assert!(runtime.store().outbox_message(event_id).is_none());
    assert!(
        runtime
            .store()
            .command_inbox_entry("projection-builder", command_id)
            .is_none()
    );
    assert_eq!(runtime.store().archived_outbox_messages().len(), 1);
    assert_eq!(runtime.store().archived_command_inbox().len(), 1);
}
