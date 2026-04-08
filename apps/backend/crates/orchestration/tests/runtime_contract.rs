use chrono::{TimeDelta, TimeZone, Utc};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

use musubi_orchestration::{
    AuthoritativeChange, CommandCompletion, CommandEnvelope, ConsumeOutcome, DeliveryOutcome,
    DeliveryReceipt, ExternalIdempotencyKey, InMemoryOrchestrationStore, NewOutboxMessage,
    OrchestrationError, OrchestrationRuntime, OrchestrationStore, OutboxAttempt,
    OutboxDeliveryStatus, ProcessingFailure, QuarantineReason, RetentionPolicy, RetryPolicy,
    SchemaCompatibilityPolicy, WriterReadSource,
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

#[test]
fn payload_hash_matches_postgres_jsonb_text_canonicalization() {
    let message = NewOutboxMessage::new(
        Uuid::from_u128(0x31),
        Uuid::from_u128(0x32),
        "settlement_case:canonical",
        "settlement_case",
        Uuid::from_u128(0x33),
        "settlement.receipt_recorded",
        1,
        json!({ "b": 1, "a": 2 }),
        ts(0),
        ts(0),
    )
    .unwrap();

    assert_eq!(
        message.payload_hash,
        "21501dbaf73f5223934d22283f01caff4132bc1de4a9550c1ed0dffeb397a323"
    );
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
async fn transient_outbox_budget_counts_total_attempts() {
    let mut runtime = runtime();
    let aggregate_id = Uuid::from_u128(0x53);
    let event_id = Uuid::from_u128(0x54);

    runtime
        .record_authoritative_write(
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "submission_requested".to_owned(),
                payload_json: json!({ "intent_id": "budgeted-outbox" }),
            },
            NewOutboxMessage::new(
                event_id,
                Uuid::from_u128(0x55),
                "settlement_case:53",
                "settlement_case",
                aggregate_id,
                "settlement.submit_action",
                1,
                json!({ "intent_id": "budgeted-outbox" }),
                ts(0),
                ts(0),
            )
            .unwrap(),
        )
        .unwrap();

    for now in [ts(0), ts(600)] {
        let outcome = runtime
            .deliver_ready_outbox("settlement-relay", now, |_| async {
                Err(ProcessingFailure::transient(
                    "provider_timeout",
                    "provider did not respond in time",
                ))
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

    let exhausted = runtime
        .deliver_ready_outbox("settlement-relay", ts(1_200), |_| async {
            Err(ProcessingFailure::transient(
                "provider_timeout",
                "provider did not respond in time",
            ))
        })
        .await
        .unwrap();

    assert_eq!(
        exhausted,
        DeliveryOutcome::Quarantined {
            event_id,
            reason: QuarantineReason::AttemptBudgetExceeded,
        }
    );
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
async fn transient_command_budget_counts_total_attempts() {
    let mut runtime = runtime();
    let command_id = Uuid::from_u128(0x69);
    let command = CommandEnvelope::new(
        command_id,
        Uuid::from_u128(0x6A),
        "projection.refresh",
        1,
        json!({ "case_id": "budgeted-command" }),
    )
    .unwrap();

    for now in [ts(0), ts(600)] {
        let outcome = runtime
            .consume_command("projection-builder", command.clone(), now, |_| async {
                Err(ProcessingFailure::transient(
                    "projection_busy",
                    "projection worker is still catching up",
                ))
            })
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

    let exhausted = runtime
        .consume_command("projection-builder", command, ts(1_200), |_| async {
            Err(ProcessingFailure::transient(
                "projection_busy",
                "projection worker is still catching up",
            ))
        })
        .await
        .unwrap();

    assert_eq!(
        exhausted,
        ConsumeOutcome::Quarantined {
            command_id,
            reason: QuarantineReason::AttemptBudgetExceeded,
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

#[test]
fn stale_outbox_retry_cannot_reopen_published_message() {
    let mut store = InMemoryOrchestrationStore::default();
    let aggregate_id = Uuid::from_u128(0x80);
    let event_id = Uuid::from_u128(0x81);

    store
        .commit_authoritative_write(
            WriterReadSource::PrimaryWriter,
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "submission_requested".to_owned(),
                payload_json: json!({ "intent_id": "stale-outbox" }),
            },
            NewOutboxMessage::new(
                event_id,
                Uuid::from_u128(0x82),
                "settlement_case:80",
                "settlement_case",
                aggregate_id,
                "settlement.submit_action",
                1,
                json!({ "intent_id": "stale-outbox" }),
                ts(0),
                ts(0),
            )
            .unwrap(),
        )
        .unwrap();

    let claimed_by_a = store
        .claim_ready_outbox(WriterReadSource::PrimaryWriter, "relay-a", ts(0), ts(300))
        .unwrap()
        .unwrap();
    let claimed_by_b = store
        .claim_ready_outbox(WriterReadSource::PrimaryWriter, "relay-b", ts(301), ts(600))
        .unwrap()
        .unwrap();

    store
        .mark_outbox_published(
            event_id,
            ts(3_600),
            DeliveryReceipt {
                external_idempotency_key: ExternalIdempotencyKey::new("provider-key-stale")
                    .unwrap(),
            },
            OutboxAttempt {
                event_id,
                attempt_number: 1,
                relay_name: "relay-b".to_owned(),
                claimed_at: claimed_by_b.claimed_at,
                claimed_until: claimed_by_b.claimed_until,
                finished_at: ts(302),
                failure_class: None,
                failure_code: None,
                failure_detail: None,
                external_idempotency_key: Some("provider-key-stale".to_owned()),
            },
        )
        .unwrap();

    store
        .schedule_outbox_retry(
            event_id,
            ts(900),
            ProcessingFailure::transient("provider_timeout", "stale retry should be ignored"),
            OutboxAttempt {
                event_id,
                attempt_number: 1,
                relay_name: "relay-a".to_owned(),
                claimed_at: claimed_by_a.claimed_at,
                claimed_until: claimed_by_a.claimed_until,
                finished_at: ts(303),
                failure_class: Some(musubi_orchestration::RetryClass::Transient),
                failure_code: Some("provider_timeout".to_owned()),
                failure_detail: Some("stale retry should be ignored".to_owned()),
                external_idempotency_key: None,
            },
        )
        .unwrap();

    let message = store.outbox_message(event_id).unwrap();
    assert_eq!(message.delivery_status, OutboxDeliveryStatus::Published);
    assert_eq!(message.attempt_count, 1);
}

#[test]
fn stale_command_retry_cannot_reopen_completed_command() {
    let mut store = InMemoryOrchestrationStore::default();
    let command_id = Uuid::from_u128(0x83);
    let command = CommandEnvelope::new(
        command_id,
        Uuid::from_u128(0x84),
        "projection.refresh",
        1,
        json!({ "case_id": "stale-command" }),
    )
    .unwrap();

    let first = store
        .begin_command(
            WriterReadSource::PrimaryWriter,
            "projection-builder",
            command.clone(),
            ts(0),
            ts(300),
        )
        .unwrap();
    let second = store
        .begin_command(
            WriterReadSource::PrimaryWriter,
            "projection-builder",
            command,
            ts(301),
            ts(600),
        )
        .unwrap();

    let first_claimed_until = match first {
        musubi_orchestration::CommandBeginOutcome::FirstSeen(entry) => entry.claimed_until.unwrap(),
        other => panic!("unexpected first outcome: {other:?}"),
    };
    let second_claimed_until = match second {
        musubi_orchestration::CommandBeginOutcome::ReadyForRetry(entry) => {
            entry.claimed_until.unwrap()
        }
        other => panic!("unexpected second outcome: {other:?}"),
    };

    store
        .complete_command(
            "projection-builder",
            command_id,
            second_claimed_until,
            ts(302),
            ts(3_600),
            CommandCompletion {
                result_type: "projected".to_owned(),
                result_json: json!({ "projection_id": "read-stale" }),
            },
        )
        .unwrap();

    store
        .schedule_command_retry(
            "projection-builder",
            command_id,
            first_claimed_until,
            ts(900),
            ProcessingFailure::transient("projection_busy", "stale retry should be ignored"),
        )
        .unwrap();

    let entry = store
        .command_inbox_entry("projection-builder", command_id)
        .unwrap();
    assert_eq!(
        entry.status,
        musubi_orchestration::CommandInboxStatus::Completed
    );
    assert_eq!(entry.attempt_count, 1);
}
