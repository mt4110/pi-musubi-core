use chrono::{TimeDelta, TimeZone, Utc};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

use musubi_orchestration::{
    AuthoritativeChange, ClaimedOutboxMessage, CommandBeginOutcome, CommandCompletion,
    CommandEnvelope, CommandInboxEntry, CommandInboxStatus, CommandKey, CommandQuarantine,
    ConsumeOutcome, DeliveryOutcome, DeliveryReceipt, ExternalIdempotencyKey,
    InMemoryOrchestrationStore, NewOutboxMessage, NewOutboxMessageSpec, OrchestrationError,
    OrchestrationRuntime, OrchestrationStore, OutboxAttempt, OutboxDeliveryStatus, OutboxMessage,
    ProcessingFailure, PruneOutcome, QuarantineReason, RetentionPolicy, RetryPolicy,
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

struct PhaseTracingStore {
    phases: Arc<Mutex<Vec<&'static str>>>,
    claimed_outbox: Option<ClaimedOutboxMessage>,
    command_begin_outcome: Option<CommandBeginOutcome>,
}

impl PhaseTracingStore {
    fn for_outbox(
        phases: Arc<Mutex<Vec<&'static str>>>,
        claimed_outbox: ClaimedOutboxMessage,
    ) -> Self {
        Self {
            phases,
            claimed_outbox: Some(claimed_outbox),
            command_begin_outcome: None,
        }
    }

    fn for_command(
        phases: Arc<Mutex<Vec<&'static str>>>,
        command_begin_outcome: CommandBeginOutcome,
    ) -> Self {
        Self {
            phases,
            claimed_outbox: None,
            command_begin_outcome: Some(command_begin_outcome),
        }
    }

    fn record(&self, phase: &'static str) {
        self.phases.lock().unwrap().push(phase);
    }
}

impl OrchestrationStore for PhaseTracingStore {
    fn commit_authoritative_write(
        &mut self,
        _source: WriterReadSource,
        _change: AuthoritativeChange,
        _message: NewOutboxMessage,
    ) -> Result<(), OrchestrationError> {
        unreachable!("phase tracing store is only used for async boundary tests")
    }

    fn claim_ready_outbox(
        &mut self,
        _source: WriterReadSource,
        _relay_name: &str,
        _now: chrono::DateTime<Utc>,
        _claimed_until: chrono::DateTime<Utc>,
    ) -> Result<Option<ClaimedOutboxMessage>, OrchestrationError> {
        self.record("claim_ready_outbox");
        Ok(self.claimed_outbox.take())
    }

    fn mark_outbox_published(
        &mut self,
        _event_id: Uuid,
        _retain_until: chrono::DateTime<Utc>,
        _receipt: DeliveryReceipt,
        _attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError> {
        self.record("mark_outbox_published");
        Ok(())
    }

    fn schedule_outbox_retry(
        &mut self,
        _event_id: Uuid,
        _retry_at: chrono::DateTime<Utc>,
        _failure: ProcessingFailure,
        _attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError> {
        unreachable!("phase tracing store is only used for successful async boundary tests")
    }

    fn quarantine_outbox(
        &mut self,
        _event_id: Uuid,
        _quarantined_at: chrono::DateTime<Utc>,
        _retain_until: chrono::DateTime<Utc>,
        _reason: QuarantineReason,
        _failure: ProcessingFailure,
        _attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError> {
        unreachable!("phase tracing store is only used for successful async boundary tests")
    }

    fn begin_command(
        &mut self,
        _source: WriterReadSource,
        _consumer_name: &str,
        _command: CommandEnvelope,
        _now: chrono::DateTime<Utc>,
        _claimed_until: chrono::DateTime<Utc>,
    ) -> Result<CommandBeginOutcome, OrchestrationError> {
        self.record("begin_command");
        self.command_begin_outcome
            .take()
            .ok_or_else(|| OrchestrationError::Database("missing command begin outcome".to_owned()))
    }

    fn complete_command(
        &mut self,
        _consumer_name: &str,
        _command_id: Uuid,
        _expected_claimed_until: chrono::DateTime<Utc>,
        _completed_at: chrono::DateTime<Utc>,
        _retain_until: chrono::DateTime<Utc>,
        _completion: CommandCompletion,
    ) -> Result<(), OrchestrationError> {
        self.record("complete_command");
        Ok(())
    }

    fn schedule_command_retry(
        &mut self,
        _consumer_name: &str,
        _command_id: Uuid,
        _expected_claimed_until: chrono::DateTime<Utc>,
        _retry_at: chrono::DateTime<Utc>,
        _failure: ProcessingFailure,
    ) -> Result<(), OrchestrationError> {
        unreachable!("phase tracing store is only used for successful async boundary tests")
    }

    fn quarantine_command(
        &mut self,
        _consumer_name: &str,
        _command_id: Uuid,
        _expected_claimed_until: chrono::DateTime<Utc>,
        _quarantine: CommandQuarantine,
    ) -> Result<(), OrchestrationError> {
        unreachable!("phase tracing store is only used for successful async boundary tests")
    }

    fn prune_coordination(
        &mut self,
        _now: chrono::DateTime<Utc>,
    ) -> Result<PruneOutcome, OrchestrationError> {
        unreachable!("phase tracing store is only used for async boundary tests")
    }
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
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id,
                idempotency_key,
                stream_key: "settlement_case:10".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.receipt_recorded".to_owned(),
                schema_version: 1,
                payload_json: json!({ "receipt_id": "receipt-1" }),
                available_at: ts(0),
                created_at: ts(0),
            })
            .unwrap(),
        )
        .unwrap();

    assert_eq!(runtime.store().authoritative_changes().len(), 1);
    let message = runtime.store().outbox_message(event_id).unwrap();
    assert_eq!(message.delivery_status, OutboxDeliveryStatus::Pending);
    assert_eq!(message.aggregate_id, aggregate_id);
}

#[tokio::test]
async fn outbox_publish_callback_runs_between_writer_phases() {
    let phases = Arc::new(Mutex::new(Vec::new()));
    let event_id = Uuid::from_u128(0x22);
    let aggregate_id = Uuid::from_u128(0x23);
    let store = PhaseTracingStore::for_outbox(
        phases.clone(),
        ClaimedOutboxMessage {
            message: OutboxMessage {
                event_id,
                idempotency_key: Uuid::from_u128(0x24),
                stream_key: "settlement_case:23".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.submit_action".to_owned(),
                schema_version: 1,
                payload_json: json!({ "intent_id": "intent-phase" }),
                payload_hash: "phase-hash".to_owned(),
                delivery_status: OutboxDeliveryStatus::Processing,
                attempt_count: 0,
                available_at: ts(0),
                created_at: ts(0),
                published_at: None,
                last_attempt_at: None,
                last_error_class: None,
                last_error_code: None,
                last_error_detail: None,
                claimed_by: Some("settlement-relay".to_owned()),
                claimed_until: Some(ts(300)),
                quarantined_at: None,
                quarantine_reason: None,
                retain_until: None,
                causal_order: 1,
                published_external_idempotency_key: None,
            },
            relay_name: "settlement-relay".to_owned(),
            claimed_at: ts(0),
            claimed_until: ts(300),
        },
    );
    let mut runtime = OrchestrationRuntime::new(
        store,
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
    );
    let phases_for_publish = phases.clone();

    let outcome = runtime
        .deliver_ready_outbox("settlement-relay", ts(0), move |_| {
            let phases = phases_for_publish.clone();
            async move {
                phases.lock().unwrap().push("publish_started");
                tokio::task::yield_now().await;
                phases.lock().unwrap().push("publish_finished");
                Ok(DeliveryReceipt {
                    external_idempotency_key: ExternalIdempotencyKey::new("provider-key-phase")
                        .unwrap(),
                })
            }
        })
        .await
        .unwrap();

    assert_eq!(outcome, DeliveryOutcome::Published { event_id });
    assert_eq!(
        phases.lock().unwrap().clone(),
        vec![
            "claim_ready_outbox",
            "publish_started",
            "publish_finished",
            "mark_outbox_published",
        ]
    );
}

#[tokio::test]
async fn command_handler_runs_between_writer_phases() {
    let phases = Arc::new(Mutex::new(Vec::new()));
    let command_id = Uuid::from_u128(0x25);
    let store = PhaseTracingStore::for_command(
        phases.clone(),
        CommandBeginOutcome::FirstSeen(CommandInboxEntry {
            key: CommandKey {
                consumer_name: "projection-builder".to_owned(),
                command_id,
            },
            source_event_id: Uuid::from_u128(0x26),
            command_type: "projection.refresh".to_owned(),
            schema_version: 1,
            payload_hash: "command-phase-hash".to_owned(),
            status: CommandInboxStatus::Processing,
            attempt_count: 0,
            first_seen_at: ts(0),
            available_at: ts(0),
            completed_at: None,
            claimed_by: Some("projection-builder".to_owned()),
            claimed_until: Some(ts(300)),
            last_error_class: None,
            last_error_code: None,
            last_error_detail: None,
            quarantined_at: None,
            quarantine_reason: None,
            result_type: None,
            result_json: None,
            retain_until: None,
        }),
    );
    let mut runtime = OrchestrationRuntime::new(
        store,
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
    );
    let phases_for_handler = phases.clone();

    let outcome = runtime
        .consume_command(
            "projection-builder",
            CommandEnvelope::new(
                command_id,
                Uuid::from_u128(0x26),
                "projection.refresh",
                1,
                json!({ "case_id": "case-phase" }),
            )
            .unwrap(),
            ts(0),
            move |_| {
                let phases = phases_for_handler.clone();
                async move {
                    phases.lock().unwrap().push("handle_started");
                    tokio::task::yield_now().await;
                    phases.lock().unwrap().push("handle_finished");
                    Ok(CommandCompletion {
                        result_type: "projected".to_owned(),
                        result_json: json!({ "projection_id": "phase-read" }),
                    })
                }
            },
        )
        .await
        .unwrap();

    assert_eq!(outcome, ConsumeOutcome::Completed { command_id });
    assert_eq!(
        phases.lock().unwrap().clone(),
        vec![
            "begin_command",
            "handle_started",
            "handle_finished",
            "complete_command",
        ]
    );
}

#[test]
fn payload_hash_matches_postgres_jsonb_text_canonicalization() {
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id: Uuid::from_u128(0x31),
        idempotency_key: Uuid::from_u128(0x32),
        stream_key: "settlement_case:canonical".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id: Uuid::from_u128(0x33),
        event_type: "settlement.receipt_recorded".to_owned(),
        schema_version: 1,
        payload_json: json!({ "b": 1, "a": 2 }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    assert_eq!(
        message.payload_hash,
        "21501dbaf73f5223934d22283f01caff4132bc1de4a9550c1ed0dffeb397a323"
    );
}

#[test]
fn authoritative_progression_rejects_read_replica_sources() {
    let mut store = InMemoryOrchestrationStore::default();
    let aggregate_id = Uuid::from_u128(0x34);
    let event_id = Uuid::from_u128(0x35);

    let write_from_replica = store.commit_authoritative_write(
        WriterReadSource::ReadReplica,
        AuthoritativeChange {
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            change_type: "receipt_recorded".to_owned(),
            payload_json: json!({ "receipt_id": "replica-write" }),
        },
        NewOutboxMessage::new(NewOutboxMessageSpec {
            event_id,
            idempotency_key: Uuid::from_u128(0x36),
            stream_key: "settlement_case:34".to_owned(),
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            event_type: "settlement.receipt_recorded".to_owned(),
            schema_version: 1,
            payload_json: json!({ "receipt_id": "replica-write" }),
            available_at: ts(0),
            created_at: ts(0),
        })
        .unwrap(),
    );

    assert_eq!(
        write_from_replica,
        Err(OrchestrationError::ReplicaReadForbidden)
    );

    store
        .commit_authoritative_write(
            WriterReadSource::PrimaryWriter,
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "receipt_recorded".to_owned(),
                payload_json: json!({ "receipt_id": "writer-only" }),
            },
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id,
                idempotency_key: Uuid::from_u128(0x37),
                stream_key: "settlement_case:34".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.receipt_recorded".to_owned(),
                schema_version: 1,
                payload_json: json!({ "receipt_id": "writer-only" }),
                available_at: ts(0),
                created_at: ts(0),
            })
            .unwrap(),
        )
        .unwrap();

    let claim_from_replica =
        store.claim_ready_outbox(WriterReadSource::ReadReplica, "relay-a", ts(1), ts(301));
    let begin_from_replica = store.begin_command(
        WriterReadSource::ReadReplica,
        "projection-builder",
        CommandEnvelope::new(
            Uuid::from_u128(0x38),
            event_id,
            "projection.refresh",
            1,
            json!({ "case_id": "replica-command" }),
        )
        .unwrap(),
        ts(1),
        ts(301),
    );

    assert_eq!(
        claim_from_replica,
        Err(OrchestrationError::ReplicaReadForbidden)
    );
    assert_eq!(
        begin_from_replica,
        Err(OrchestrationError::ReplicaReadForbidden)
    );
}

#[test]
fn duplicate_outbox_idempotency_key_does_not_duplicate_truth() {
    let mut store = InMemoryOrchestrationStore::default();
    let aggregate_id = Uuid::from_u128(0x39);
    let first_event_id = Uuid::from_u128(0x3A);
    let duplicate_event_id = Uuid::from_u128(0x3B);
    let idempotency_key = Uuid::from_u128(0x3C);

    store
        .commit_authoritative_write(
            WriterReadSource::PrimaryWriter,
            AuthoritativeChange {
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                change_type: "receipt_recorded".to_owned(),
                payload_json: json!({ "receipt_id": "first" }),
            },
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id: first_event_id,
                idempotency_key,
                stream_key: "settlement_case:39".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.receipt_recorded".to_owned(),
                schema_version: 1,
                payload_json: json!({ "receipt_id": "first" }),
                available_at: ts(0),
                created_at: ts(0),
            })
            .unwrap(),
        )
        .unwrap();

    let duplicate = store.commit_authoritative_write(
        WriterReadSource::PrimaryWriter,
        AuthoritativeChange {
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            change_type: "receipt_recorded".to_owned(),
            payload_json: json!({ "receipt_id": "duplicate" }),
        },
        NewOutboxMessage::new(NewOutboxMessageSpec {
            event_id: duplicate_event_id,
            idempotency_key,
            stream_key: "settlement_case:39".to_owned(),
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            event_type: "settlement.receipt_recorded".to_owned(),
            schema_version: 1,
            payload_json: json!({ "receipt_id": "duplicate" }),
            available_at: ts(1),
            created_at: ts(1),
        })
        .unwrap(),
    );

    assert_eq!(
        duplicate,
        Err(OrchestrationError::IdempotencyKeyAlreadyExists { idempotency_key })
    );
    assert_eq!(store.authoritative_changes().len(), 1);
    assert!(store.outbox_message(first_event_id).is_some());
    assert!(store.outbox_message(duplicate_event_id).is_none());
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
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id,
                idempotency_key: Uuid::from_u128(0x52),
                stream_key: "settlement_case:50".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.submit_action".to_owned(),
                schema_version: 1,
                payload_json: json!({ "intent_id": "intent-1" }),
                available_at: ts(0),
                created_at: ts(0),
            })
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
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id,
                idempotency_key: Uuid::from_u128(0x55),
                stream_key: "settlement_case:53".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.submit_action".to_owned(),
                schema_version: 1,
                payload_json: json!({ "intent_id": "budgeted-outbox" }),
                available_at: ts(0),
                created_at: ts(0),
            })
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
            reason: QuarantineReason::CompatibilityWindowExpired,
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
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id,
                idempotency_key: Uuid::from_u128(0x68),
                stream_key: "settlement_case:66".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.submit_action".to_owned(),
                schema_version: 99,
                payload_json: json!({ "intent_id": "future-schema" }),
                available_at: ts(0),
                created_at: ts(0),
            })
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
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id,
                idempotency_key: Uuid::from_u128(0x73),
                stream_key: "settlement_case:70".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.submit_action".to_owned(),
                schema_version: 1,
                payload_json: json!({ "intent_id": "intent-2" }),
                available_at: ts(0),
                created_at: ts(0),
            })
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
            NewOutboxMessage::new(NewOutboxMessageSpec {
                event_id,
                idempotency_key: Uuid::from_u128(0x82),
                stream_key: "settlement_case:80".to_owned(),
                aggregate_type: "settlement_case".to_owned(),
                aggregate_id,
                event_type: "settlement.submit_action".to_owned(),
                schema_version: 1,
                payload_json: json!({ "intent_id": "stale-outbox" }),
                available_at: ts(0),
                created_at: ts(0),
            })
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

    let stale_retry = store
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
        .unwrap_err();

    assert_eq!(
        stale_retry,
        OrchestrationError::StaleOutboxClaim { event_id }
    );

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

    let stale_retry = store
        .schedule_command_retry(
            "projection-builder",
            command_id,
            first_claimed_until,
            ts(900),
            ProcessingFailure::transient("projection_busy", "stale retry should be ignored"),
        )
        .unwrap_err();

    assert_eq!(
        stale_retry,
        OrchestrationError::StaleCommandClaim {
            consumer_name: "projection-builder".to_owned(),
            command_id,
        }
    );

    let entry = store
        .command_inbox_entry("projection-builder", command_id)
        .unwrap();
    assert_eq!(
        entry.status,
        musubi_orchestration::CommandInboxStatus::Completed
    );
    assert_eq!(entry.attempt_count, 1);
}
