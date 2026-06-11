use chrono::{TimeZone, Utc};
use serde_json::json;
use tokio_postgres::NoTls;
use uuid::Uuid;

use musubi_orchestration::{
    AuthoritativeSqlCommand, CommandBeginOutcome, CommandEnvelope, CommandKey, NewOutboxMessage,
    NewOutboxMessageSpec, OrchestrationError, PostgresOrchestrationStore, SqlParam,
};

fn ts(seconds: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_100_000 + seconds, 0).unwrap()
}

#[tokio::test]
async fn postgres_helpers_keep_truth_and_outbox_in_same_transaction() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");
    tx.batch_execute(
        "
        CREATE TEMP TABLE authoritative_facts (
            fact_id UUID PRIMARY KEY,
            fact_kind TEXT NOT NULL
        )
        ",
    )
    .await
    .expect("failed to create temp authoritative_facts table");

    let fact_id = Uuid::from_u128(0x700);
    let event_id = Uuid::from_u128(0x701);
    let command_id = Uuid::from_u128(0x702);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x703),
        stream_key: "settlement_case:700".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id: fact_id,
        event_type: "settlement.receipt_recorded".to_owned(),
        schema_version: 1,
        payload_json: json!({ "fact_id": fact_id }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    let authoritative_commands = [AuthoritativeSqlCommand {
        statement: "INSERT INTO authoritative_facts (fact_id, fact_kind) VALUES ($1, $2)",
        params: vec![
            &fact_id as SqlParam<'_>,
            &"receipt_recorded" as SqlParam<'_>,
        ],
    }];

    PostgresOrchestrationStore::record_authoritative_write(&tx, &authoritative_commands, &message)
        .await
        .expect("same-tx authoritative write + outbox insert should succeed");

    let fact_count: i64 = tx
        .query_one("SELECT COUNT(*) AS count FROM authoritative_facts", &[])
        .await
        .expect("failed to count authoritative_facts")
        .get("count");
    let outbox_count: i64 = tx
        .query_one("SELECT COUNT(*) AS count FROM outbox.events", &[])
        .await
        .expect("failed to count outbox events")
        .get("count");

    assert_eq!(fact_count, 1);
    assert_eq!(outbox_count, 1);

    let command = CommandEnvelope::new(
        command_id,
        event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": fact_id }),
    )
    .unwrap();

    let first = PostgresOrchestrationStore::begin_command(
        &tx,
        "projection-builder",
        &command,
        ts(10),
        ts(10 + 300),
    )
    .await
    .expect("first inbox write should succeed");
    let second = PostgresOrchestrationStore::begin_command(
        &tx,
        "projection-builder",
        &command,
        ts(11),
        ts(11 + 300),
    )
    .await
    .expect("duplicate inbox write should be handled");

    assert!(matches!(first, CommandBeginOutcome::FirstSeen(_)));
    assert!(matches!(
        second,
        CommandBeginOutcome::Deferred(entry) if entry.available_at == ts(10 + 300)
    ));

    let conflicting = CommandEnvelope::new(
        command_id,
        event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": "different-payload" }),
    )
    .unwrap();

    let conflict = PostgresOrchestrationStore::begin_command(
        &tx,
        "projection-builder",
        &conflicting,
        ts(12),
        ts(12 + 300),
    )
    .await;

    assert_eq!(
        conflict,
        Err(OrchestrationError::ConflictingCommandEnvelope {
            consumer_name: "projection-builder".to_owned(),
            command_id,
        })
    );

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_helper_rejects_empty_authoritative_write_batch() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");

    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id: Uuid::from_u128(0x706),
        idempotency_key: Uuid::from_u128(0x707),
        stream_key: "settlement_case:706".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id: Uuid::from_u128(0x708),
        event_type: "settlement.receipt_recorded".to_owned(),
        schema_version: 1,
        payload_json: json!({ "fact_id": "empty-batch" }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    let error = PostgresOrchestrationStore::record_authoritative_write(&tx, &[], &message)
        .await
        .unwrap_err();

    assert_eq!(error, OrchestrationError::EmptyAuthoritativeWriteBatch);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_authoritative_write_rolls_back_truth_and_outbox_together() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let mut tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");
    tx.batch_execute(
        "
        CREATE TEMP TABLE authoritative_facts_rollback (
            fact_id UUID PRIMARY KEY,
            fact_kind TEXT NOT NULL
        )
        ",
    )
    .await
    .expect("failed to create temp authoritative_facts_rollback table");

    let fact_id = Uuid::from_u128(0x709);
    let event_id = Uuid::from_u128(0x70a);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x70b),
        stream_key: "settlement_case:709".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id: fact_id,
        event_type: "settlement.receipt_recorded".to_owned(),
        schema_version: 1,
        payload_json: json!({ "fact_id": fact_id }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    let authoritative_commands = [AuthoritativeSqlCommand {
        statement: "INSERT INTO authoritative_facts_rollback (fact_id, fact_kind) VALUES ($1, $2)",
        params: vec![
            &fact_id as SqlParam<'_>,
            &"receipt_recorded" as SqlParam<'_>,
        ],
    }];

    let rollback_tx = tx
        .transaction()
        .await
        .expect("failed to open rollback transaction");
    PostgresOrchestrationStore::record_authoritative_write(
        &rollback_tx,
        &authoritative_commands,
        &message,
    )
    .await
    .expect("same-tx authoritative write + outbox insert should succeed");
    rollback_tx
        .rollback()
        .await
        .expect("rollback should discard truth and outbox rows");

    let fact_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM authoritative_facts_rollback
            WHERE fact_id = $1
            ",
            &[&fact_id],
        )
        .await
        .expect("failed to count rolled-back authoritative facts")
        .get("count");
    let outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.events
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("failed to count rolled-back outbox events")
        .get("count");

    assert_eq!(fact_count, 0);
    assert_eq!(outbox_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_duplicate_outbox_idempotency_rolls_back_authoritative_sql() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let mut tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");
    tx.batch_execute(
        "
        CREATE TEMP TABLE authoritative_facts_duplicate (
            fact_id UUID PRIMARY KEY,
            fact_kind TEXT NOT NULL
        )
        ",
    )
    .await
    .expect("failed to create temp authoritative_facts_duplicate table");

    let first_fact_id = Uuid::from_u128(0x70c);
    let second_fact_id = Uuid::from_u128(0x70d);
    let first_event_id = Uuid::from_u128(0x70e);
    let second_event_id = Uuid::from_u128(0x70f);
    let idempotency_key = Uuid::from_u128(0x710);

    let first_message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id: first_event_id,
        idempotency_key,
        stream_key: "settlement_case:70c".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id: first_fact_id,
        event_type: "settlement.receipt_recorded".to_owned(),
        schema_version: 1,
        payload_json: json!({ "fact_id": first_fact_id }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();
    let second_message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id: second_event_id,
        idempotency_key,
        stream_key: "settlement_case:70d".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id: second_fact_id,
        event_type: "settlement.receipt_recorded".to_owned(),
        schema_version: 1,
        payload_json: json!({ "fact_id": second_fact_id }),
        available_at: ts(1),
        created_at: ts(1),
    })
    .unwrap();

    let first_commands = [AuthoritativeSqlCommand {
        statement: "INSERT INTO authoritative_facts_duplicate (fact_id, fact_kind) VALUES ($1, $2)",
        params: vec![
            &first_fact_id as SqlParam<'_>,
            &"receipt_recorded" as SqlParam<'_>,
        ],
    }];
    let second_commands = [AuthoritativeSqlCommand {
        statement: "INSERT INTO authoritative_facts_duplicate (fact_id, fact_kind) VALUES ($1, $2)",
        params: vec![
            &second_fact_id as SqlParam<'_>,
            &"duplicate_attempt" as SqlParam<'_>,
        ],
    }];

    PostgresOrchestrationStore::record_authoritative_write(&tx, &first_commands, &first_message)
        .await
        .expect("initial authoritative write + outbox insert should succeed");

    let duplicate_tx = tx
        .transaction()
        .await
        .expect("failed to open duplicate savepoint");
    let duplicate_error = PostgresOrchestrationStore::record_authoritative_write(
        &duplicate_tx,
        &second_commands,
        &second_message,
    )
    .await
    .expect_err("duplicate outbox idempotency key should fail");
    assert!(matches!(duplicate_error, OrchestrationError::Database(_)));
    duplicate_tx
        .rollback()
        .await
        .expect("duplicate savepoint rollback should discard the attempted authoritative SQL");

    let first_fact_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM authoritative_facts_duplicate
            WHERE fact_id = $1
            ",
            &[&first_fact_id],
        )
        .await
        .expect("failed to count first authoritative fact")
        .get("count");
    let second_fact_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM authoritative_facts_duplicate
            WHERE fact_id = $1
            ",
            &[&second_fact_id],
        )
        .await
        .expect("failed to count second authoritative fact")
        .get("count");
    let first_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.events
            WHERE event_id = $1
            ",
            &[&first_event_id],
        )
        .await
        .expect("failed to count first outbox event")
        .get("count");
    let second_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.events
            WHERE event_id = $1
            ",
            &[&second_event_id],
        )
        .await
        .expect("failed to count second outbox event")
        .get("count");

    assert_eq!(first_fact_count, 1);
    assert_eq!(second_fact_count, 0);
    assert_eq!(first_outbox_count, 1);
    assert_eq!(second_outbox_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_archives_terminal_coordination_rows() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");

    let aggregate_id = Uuid::from_u128(0x711);
    let event_id = Uuid::from_u128(0x712);
    let command_id = Uuid::from_u128(0x713);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x714),
        stream_key: "settlement_case:711".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id,
        event_type: "settlement.submit_action".to_owned(),
        schema_version: 1,
        payload_json: json!({ "intent_id": "intent-prune" }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
        .await
        .expect("failed to insert prune outbox message");
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = 'published',
            attempt_count = 1,
            published_at = $2,
            retain_until = $3,
            published_external_idempotency_key = $4
        WHERE event_id = $1
        ",
        &[&event_id, &ts(10), &ts(100), &"provider-key-prune"],
    )
    .await
    .expect("failed to mark outbox event terminal");
    tx.execute(
        "
        INSERT INTO outbox.outbox_attempts (
            event_id,
            attempt_number,
            relay_name,
            claimed_at,
            claimed_until,
            finished_at,
            failure_class,
            failure_code,
            failure_detail,
            external_idempotency_key
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL, NULL, $7)
        ",
        &[
            &event_id,
            &1_i32,
            &"settlement-relay",
            &ts(0),
            &ts(300),
            &ts(10),
            &"provider-key-prune",
        ],
    )
    .await
    .expect("failed to insert outbox attempt");

    let command = CommandEnvelope::new(
        command_id,
        event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": aggregate_id }),
    )
    .unwrap();
    tx.execute(
        "
        INSERT INTO outbox.command_inbox (
            inbox_entry_id,
            consumer_name,
            command_id,
            source_event_id,
            payload_checksum,
            received_at,
            status,
            available_at,
            attempt_count,
            command_type,
            schema_version,
            completed_at,
            result_type,
            result_json,
            retain_until
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'completed', $6, 1, $7, $8, $9, $10, $11, $12)
        ",
        &[
            &Uuid::new_v4(),
            &"projection-builder",
            &command_id,
            &event_id,
            &command.payload_hash,
            &ts(20),
            &command.command_type,
            &command.schema_version,
            &ts(30),
            &"projected",
            &json!({ "projection_id": "projection-prune" }),
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert completed command inbox row");

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune coordination rows");

    assert_eq!(outcome.pruned_outbox_event_ids, vec![event_id]);
    assert_eq!(
        outcome.pruned_command_keys,
        vec![CommandKey {
            consumer_name: "projection-builder".to_owned(),
            command_id,
        }]
    );

    let hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox events")
        .get("count");
    let archived_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_event_archive WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count archived outbox events")
        .get("count");
    let hot_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox attempts")
        .get("count");
    let archived_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempt_archive WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count archived outbox attempts")
        .get("count");
    let hot_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to count hot command inbox rows")
        .get("count");
    let archived_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to count archived command inbox rows")
        .get("count");

    assert_eq!(hot_outbox_count, 0);
    assert_eq!(archived_outbox_count, 1);
    assert_eq!(hot_attempt_count, 0);
    assert_eq!(archived_attempt_count, 1);
    assert_eq!(hot_command_count, 0);
    assert_eq!(archived_command_count, 1);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_preserves_terminal_archive_payloads() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");

    let aggregate_id = Uuid::from_u128(0x71f);
    let event_id = Uuid::from_u128(0x720);
    let command_id = Uuid::from_u128(0x721);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x722),
        stream_key: "settlement_case:archive-payload".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id,
        event_type: "settlement.submit_action".to_owned(),
        schema_version: 1,
        payload_json: json!({
            "intent_id": "intent-archive-payload",
            "retention_probe": { "kind": "terminal_archive_payload" }
        }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
        .await
        .expect("failed to insert archive payload outbox message");
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = 'published',
            attempt_count = 1,
            published_at = $2,
            last_attempt_at = $2,
            retain_until = $3,
            published_external_idempotency_key = $4
        WHERE event_id = $1
        ",
        &[
            &event_id,
            &ts(10),
            &ts(100),
            &"provider-key-archive-payload",
        ],
    )
    .await
    .expect("failed to mark archive payload outbox event terminal");
    let causal_order: i64 = tx
        .query_one(
            "SELECT causal_order FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to read source outbox causal order")
        .get("causal_order");
    tx.execute(
        "
        INSERT INTO outbox.outbox_attempts (
            event_id,
            attempt_number,
            relay_name,
            claimed_at,
            claimed_until,
            finished_at,
            failure_class,
            failure_code,
            failure_detail,
            external_idempotency_key
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL, NULL, $7)
        ",
        &[
            &event_id,
            &1_i32,
            &"settlement-relay",
            &ts(0),
            &ts(300),
            &ts(10),
            &"provider-key-archive-payload",
        ],
    )
    .await
    .expect("failed to insert archive payload outbox attempt");

    let command = CommandEnvelope::new(
        command_id,
        event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": aggregate_id }),
    )
    .unwrap();
    let result_json = json!({
        "projection_id": "projection-archive-payload",
        "archived_payload_seen": true
    });
    tx.execute(
        "
        INSERT INTO outbox.command_inbox (
            inbox_entry_id,
            consumer_name,
            command_id,
            source_event_id,
            payload_checksum,
            received_at,
            status,
            available_at,
            attempt_count,
            command_type,
            schema_version,
            completed_at,
            result_type,
            result_json,
            retain_until
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'completed', $6, 1, $7, $8, $9, $10, $11, $12)
        ",
        &[
            &Uuid::new_v4(),
            &"projection-builder",
            &command_id,
            &event_id,
            &command.payload_hash,
            &ts(20),
            &command.command_type,
            &command.schema_version,
            &ts(30),
            &"projected",
            &result_json,
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert archive payload command inbox row");

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune archive payload coordination rows");

    assert_eq!(outcome.pruned_outbox_event_ids, vec![event_id]);
    assert_eq!(
        outcome.pruned_command_keys,
        vec![CommandKey {
            consumer_name: "projection-builder".to_owned(),
            command_id,
        }]
    );

    let archived_event = tx
        .query_one(
            "
            SELECT
                archived_at,
                stream_key,
                aggregate_type,
                aggregate_id,
                event_type,
                schema_version,
                payload_json,
                payload_hash,
                final_status,
                attempt_count,
                causal_order,
                available_at,
                created_at,
                published_at,
                last_attempt_at,
                retain_until,
                published_external_idempotency_key
            FROM outbox.outbox_event_archive
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("failed to read archived outbox event");
    assert_eq!(
        archived_event.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_event
            .get::<_, Option<String>>("stream_key")
            .as_deref(),
        Some(message.stream_key.as_str())
    );
    assert_eq!(
        archived_event.get::<_, String>("aggregate_type"),
        message.aggregate_type.as_str()
    );
    assert_eq!(archived_event.get::<_, Uuid>("aggregate_id"), aggregate_id);
    assert_eq!(
        archived_event.get::<_, String>("event_type"),
        message.event_type.as_str()
    );
    assert_eq!(archived_event.get::<_, i32>("schema_version"), 1);
    assert_eq!(
        archived_event.get::<_, serde_json::Value>("payload_json"),
        message.payload_json.clone()
    );
    assert_eq!(
        archived_event
            .get::<_, Option<String>>("payload_hash")
            .as_deref(),
        Some(message.payload_hash.as_str())
    );
    assert_eq!(archived_event.get::<_, String>("final_status"), "published");
    assert_eq!(archived_event.get::<_, i32>("attempt_count"), 1);
    assert_eq!(archived_event.get::<_, i64>("causal_order"), causal_order);
    assert_eq!(
        archived_event.get::<_, chrono::DateTime<Utc>>("available_at"),
        ts(0)
    );
    assert_eq!(
        archived_event.get::<_, chrono::DateTime<Utc>>("created_at"),
        ts(0)
    );
    assert_eq!(
        archived_event.get::<_, Option<chrono::DateTime<Utc>>>("published_at"),
        Some(ts(10))
    );
    assert_eq!(
        archived_event.get::<_, Option<chrono::DateTime<Utc>>>("last_attempt_at"),
        Some(ts(10))
    );
    assert_eq!(
        archived_event.get::<_, Option<chrono::DateTime<Utc>>>("retain_until"),
        Some(ts(100))
    );
    assert_eq!(
        archived_event
            .get::<_, Option<String>>("published_external_idempotency_key")
            .as_deref(),
        Some("provider-key-archive-payload")
    );

    let archived_attempt = tx
        .query_one(
            "
            SELECT
                archived_at,
                relay_name,
                claimed_at,
                claimed_until,
                finished_at,
                failure_class,
                failure_code,
                failure_detail,
                external_idempotency_key
            FROM outbox.outbox_attempt_archive
            WHERE event_id = $1
              AND attempt_number = $2
            ",
            &[&event_id, &1_i32],
        )
        .await
        .expect("failed to read archived outbox attempt");
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_attempt.get::<_, String>("relay_name"),
        "settlement-relay"
    );
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("claimed_at"),
        ts(0)
    );
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("claimed_until"),
        ts(300)
    );
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("finished_at"),
        ts(10)
    );
    assert_eq!(
        archived_attempt.get::<_, Option<String>>("failure_class"),
        None
    );
    assert_eq!(
        archived_attempt.get::<_, Option<String>>("failure_code"),
        None
    );
    assert_eq!(
        archived_attempt.get::<_, Option<String>>("failure_detail"),
        None
    );
    assert_eq!(
        archived_attempt
            .get::<_, Option<String>>("external_idempotency_key")
            .as_deref(),
        Some("provider-key-archive-payload")
    );

    let archived_command = tx
        .query_one(
            "
            SELECT
                source_event_id,
                archived_at,
                command_type,
                schema_version,
                status,
                attempt_count,
                payload_checksum,
                received_at,
                completed_at,
                result_type,
                result_json,
                retain_until
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to read archived command inbox row");
    assert_eq!(archived_command.get::<_, Uuid>("source_event_id"), event_id);
    assert_eq!(
        archived_command.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_command.get::<_, String>("command_type"),
        command.command_type.as_str()
    );
    assert_eq!(
        archived_command.get::<_, i32>("schema_version"),
        command.schema_version
    );
    assert_eq!(archived_command.get::<_, String>("status"), "completed");
    assert_eq!(archived_command.get::<_, i32>("attempt_count"), 1);
    assert_eq!(
        archived_command
            .get::<_, Option<String>>("payload_checksum")
            .as_deref(),
        Some(command.payload_hash.as_str())
    );
    assert_eq!(
        archived_command.get::<_, chrono::DateTime<Utc>>("received_at"),
        ts(20)
    );
    assert_eq!(
        archived_command.get::<_, Option<chrono::DateTime<Utc>>>("completed_at"),
        Some(ts(30))
    );
    assert_eq!(
        archived_command
            .get::<_, Option<String>>("result_type")
            .as_deref(),
        Some("projected")
    );
    assert_eq!(
        archived_command.get::<_, Option<serde_json::Value>>("result_json"),
        Some(result_json)
    );
    assert_eq!(
        archived_command.get::<_, Option<chrono::DateTime<Utc>>>("retain_until"),
        Some(ts(100))
    );

    let hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox events after archive payload prune")
        .get("count");
    let hot_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox attempts after archive payload prune")
        .get("count");
    let hot_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to count hot command rows after archive payload prune")
        .get("count");

    assert_eq!(hot_outbox_count, 0);
    assert_eq!(hot_attempt_count, 0);
    assert_eq!(hot_command_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_preserves_terminal_quarantine_archive_diagnostics() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");

    let aggregate_id = Uuid::from_u128(0x723);
    let event_id = Uuid::from_u128(0x724);
    let command_id = Uuid::from_u128(0x725);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x726),
        stream_key: "settlement_case:quarantine-diagnostics".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id,
        event_type: "settlement.submit_action".to_owned(),
        schema_version: 1,
        payload_json: json!({
            "intent_id": "intent-quarantine-diagnostics",
            "retention_probe": { "kind": "terminal_quarantine_diagnostics" }
        }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
        .await
        .expect("failed to insert quarantine diagnostics outbox message");
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = 'quarantined',
            attempt_count = 2,
            quarantined_at = $2,
            last_attempt_at = $3,
            last_error_class = $4,
            last_error_code = $5,
            last_error_detail = $6,
            quarantine_reason = $7,
            retain_until = $8
        WHERE event_id = $1
        ",
        &[
            &event_id,
            &ts(40),
            &ts(30),
            &"permanent",
            &"poison_payload",
            &"payload cannot be parsed by relay",
            &"poison_pill",
            &ts(100),
        ],
    )
    .await
    .expect("failed to mark outbox event quarantined");
    tx.execute(
        "
        INSERT INTO outbox.outbox_attempts (
            event_id,
            attempt_number,
            relay_name,
            claimed_at,
            claimed_until,
            finished_at,
            failure_class,
            failure_code,
            failure_detail,
            external_idempotency_key
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ",
        &[
            &event_id,
            &2_i32,
            &"settlement-relay",
            &ts(20),
            &ts(320),
            &ts(30),
            &"permanent",
            &"poison_payload",
            &"payload cannot be parsed by relay",
            &"provider-key-quarantine-diagnostics",
        ],
    )
    .await
    .expect("failed to insert quarantined outbox attempt");

    let command = CommandEnvelope::new(
        command_id,
        event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": aggregate_id }),
    )
    .unwrap();
    tx.execute(
        "
        INSERT INTO outbox.command_inbox (
            inbox_entry_id,
            consumer_name,
            command_id,
            source_event_id,
            payload_checksum,
            received_at,
            status,
            available_at,
            attempt_count,
            command_type,
            schema_version,
            last_error_class,
            last_error_code,
            last_error_detail,
            quarantine_reason,
            retain_until
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'quarantined', $6, 2, $7, $8, $9, $10, $11, $12, $13)
        ",
        &[
            &Uuid::new_v4(),
            &"projection-builder",
            &command_id,
            &event_id,
            &command.payload_hash,
            &ts(25),
            &command.command_type,
            &command.schema_version,
            &"permanent",
            &"poison_command",
            &"command payload rejected by projection worker",
            &"poison_pill",
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert quarantined command inbox row");

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune quarantine diagnostics coordination rows");

    assert_eq!(outcome.pruned_outbox_event_ids, vec![event_id]);
    assert_eq!(
        outcome.pruned_command_keys,
        vec![CommandKey {
            consumer_name: "projection-builder".to_owned(),
            command_id,
        }]
    );

    let archived_event = tx
        .query_one(
            "
            SELECT
                archived_at,
                final_status,
                attempt_count,
                quarantined_at,
                last_attempt_at,
                last_error_class,
                last_error_code,
                last_error_detail,
                quarantine_reason,
                retain_until
            FROM outbox.outbox_event_archive
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("failed to read quarantined archived outbox event");
    assert_eq!(
        archived_event.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_event.get::<_, String>("final_status"),
        "quarantined"
    );
    assert_eq!(archived_event.get::<_, i32>("attempt_count"), 2);
    assert_eq!(
        archived_event.get::<_, Option<chrono::DateTime<Utc>>>("quarantined_at"),
        Some(ts(40))
    );
    assert_eq!(
        archived_event.get::<_, Option<chrono::DateTime<Utc>>>("last_attempt_at"),
        Some(ts(30))
    );
    assert_eq!(
        archived_event
            .get::<_, Option<String>>("last_error_class")
            .as_deref(),
        Some("permanent")
    );
    assert_eq!(
        archived_event
            .get::<_, Option<String>>("last_error_code")
            .as_deref(),
        Some("poison_payload")
    );
    assert_eq!(
        archived_event
            .get::<_, Option<String>>("last_error_detail")
            .as_deref(),
        Some("payload cannot be parsed by relay")
    );
    assert_eq!(
        archived_event
            .get::<_, Option<String>>("quarantine_reason")
            .as_deref(),
        Some("poison_pill")
    );
    assert_eq!(
        archived_event.get::<_, Option<chrono::DateTime<Utc>>>("retain_until"),
        Some(ts(100))
    );

    let archived_attempt = tx
        .query_one(
            "
            SELECT
                archived_at,
                relay_name,
                claimed_at,
                claimed_until,
                finished_at,
                failure_class,
                failure_code,
                failure_detail,
                external_idempotency_key
            FROM outbox.outbox_attempt_archive
            WHERE event_id = $1
              AND attempt_number = $2
            ",
            &[&event_id, &2_i32],
        )
        .await
        .expect("failed to read quarantined archived outbox attempt");
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_attempt.get::<_, String>("relay_name"),
        "settlement-relay"
    );
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("claimed_at"),
        ts(20)
    );
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("claimed_until"),
        ts(320)
    );
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("finished_at"),
        ts(30)
    );
    assert_eq!(
        archived_attempt
            .get::<_, Option<String>>("failure_class")
            .as_deref(),
        Some("permanent")
    );
    assert_eq!(
        archived_attempt
            .get::<_, Option<String>>("failure_code")
            .as_deref(),
        Some("poison_payload")
    );
    assert_eq!(
        archived_attempt
            .get::<_, Option<String>>("failure_detail")
            .as_deref(),
        Some("payload cannot be parsed by relay")
    );
    assert_eq!(
        archived_attempt
            .get::<_, Option<String>>("external_idempotency_key")
            .as_deref(),
        Some("provider-key-quarantine-diagnostics")
    );

    let archived_command = tx
        .query_one(
            "
            SELECT
                source_event_id,
                archived_at,
                command_type,
                schema_version,
                status,
                attempt_count,
                payload_checksum,
                received_at,
                last_error_class,
                last_error_code,
                last_error_detail,
                quarantine_reason,
                retain_until
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to read quarantined archived command inbox row");
    assert_eq!(archived_command.get::<_, Uuid>("source_event_id"), event_id);
    assert_eq!(
        archived_command.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_command.get::<_, String>("command_type"),
        command.command_type.as_str()
    );
    assert_eq!(
        archived_command.get::<_, i32>("schema_version"),
        command.schema_version
    );
    assert_eq!(archived_command.get::<_, String>("status"), "quarantined");
    assert_eq!(archived_command.get::<_, i32>("attempt_count"), 2);
    assert_eq!(
        archived_command
            .get::<_, Option<String>>("payload_checksum")
            .as_deref(),
        Some(command.payload_hash.as_str())
    );
    assert_eq!(
        archived_command.get::<_, chrono::DateTime<Utc>>("received_at"),
        ts(25)
    );
    assert_eq!(
        archived_command
            .get::<_, Option<String>>("last_error_class")
            .as_deref(),
        Some("permanent")
    );
    assert_eq!(
        archived_command
            .get::<_, Option<String>>("last_error_code")
            .as_deref(),
        Some("poison_command")
    );
    assert_eq!(
        archived_command
            .get::<_, Option<String>>("last_error_detail")
            .as_deref(),
        Some("command payload rejected by projection worker")
    );
    assert_eq!(
        archived_command
            .get::<_, Option<String>>("quarantine_reason")
            .as_deref(),
        Some("poison_pill")
    );
    assert_eq!(
        archived_command.get::<_, Option<chrono::DateTime<Utc>>>("retain_until"),
        Some(ts(100))
    );

    let hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot quarantined outbox events after prune")
        .get("count");
    let hot_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot quarantined outbox attempts after prune")
        .get("count");
    let hot_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to count hot quarantined command rows after prune")
        .get("count");

    assert_eq!(hot_outbox_count, 0);
    assert_eq!(hot_attempt_count, 0);
    assert_eq!(hot_command_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_preserves_nonterminal_coordination_rows() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");

    let pending_event_id = Uuid::from_u128(0x717);
    let processing_event_id = Uuid::from_u128(0x718);
    let pending_command_id = Uuid::from_u128(0x719);
    let processing_command_id = Uuid::from_u128(0x71a);

    for (event_id, idempotency_key, aggregate_id, label) in [
        (
            pending_event_id,
            Uuid::from_u128(0x71b),
            Uuid::from_u128(0x71c),
            "pending-prune",
        ),
        (
            processing_event_id,
            Uuid::from_u128(0x71d),
            Uuid::from_u128(0x71e),
            "processing-prune",
        ),
    ] {
        let message = NewOutboxMessage::new(NewOutboxMessageSpec {
            event_id,
            idempotency_key,
            stream_key: format!("settlement_case:{label}"),
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            event_type: "settlement.submit_action".to_owned(),
            schema_version: 1,
            payload_json: json!({ "intent_id": label }),
            available_at: ts(0),
            created_at: ts(0),
        })
        .unwrap();
        PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
            .await
            .expect("failed to insert nonterminal outbox message");
    }

    tx.execute(
        "
        UPDATE outbox.events
        SET retain_until = $2
        WHERE event_id = $1
        ",
        &[&pending_event_id, &ts(100)],
    )
    .await
    .expect("failed to mark pending outbox retain_until");
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = 'processing',
            claimed_by = $2,
            claimed_until = $3,
            retain_until = $4
        WHERE event_id = $1
        ",
        &[
            &processing_event_id,
            &"settlement-relay",
            &ts(300),
            &ts(100),
        ],
    )
    .await
    .expect("failed to mark processing outbox retain_until");

    let pending_command = CommandEnvelope::new(
        pending_command_id,
        pending_event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": "pending-prune" }),
    )
    .unwrap();
    let processing_command = CommandEnvelope::new(
        processing_command_id,
        processing_event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": "processing-prune" }),
    )
    .unwrap();

    for (command, status, claimed_by, claimed_until) in [
        (&pending_command, "pending", None, None),
        (
            &processing_command,
            "processing",
            Some("projection-builder"),
            Some(ts(300)),
        ),
    ] {
        tx.execute(
            "
            INSERT INTO outbox.command_inbox (
                inbox_entry_id,
                consumer_name,
                command_id,
                source_event_id,
                payload_checksum,
                received_at,
                status,
                available_at,
                attempt_count,
                command_type,
                schema_version,
                claimed_by,
                claimed_until,
                retain_until
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $6, 0, $8, $9, $10, $11, $12)
            ",
            &[
                &Uuid::new_v4(),
                &"projection-builder",
                &command.command_id,
                &command.source_event_id,
                &command.payload_hash,
                &ts(0),
                &status,
                &command.command_type,
                &command.schema_version,
                &claimed_by,
                &claimed_until,
                &ts(100),
            ],
        )
        .await
        .expect("failed to insert nonterminal command inbox row");
    }

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune coordination rows");

    assert!(outcome.pruned_outbox_event_ids.is_empty());
    assert!(outcome.pruned_command_keys.is_empty());

    let hot_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.events
            WHERE event_id IN ($1, $2)
            ",
            &[&pending_event_id, &processing_event_id],
        )
        .await
        .expect("failed to count nonterminal hot outbox rows")
        .get("count");
    let archived_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_event_archive
            WHERE event_id IN ($1, $2)
            ",
            &[&pending_event_id, &processing_event_id],
        )
        .await
        .expect("failed to count nonterminal archived outbox rows")
        .get("count");
    let hot_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id IN ($2, $3)
            ",
            &[
                &"projection-builder",
                &pending_command_id,
                &processing_command_id,
            ],
        )
        .await
        .expect("failed to count nonterminal hot command inbox rows")
        .get("count");
    let archived_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id IN ($2, $3)
            ",
            &[
                &"projection-builder",
                &pending_command_id,
                &processing_command_id,
            ],
        )
        .await
        .expect("failed to count nonterminal archived command inbox rows")
        .get("count");

    assert_eq!(hot_outbox_count, 2);
    assert_eq!(archived_outbox_count, 0);
    assert_eq!(hot_command_count, 2);
    assert_eq!(archived_command_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_begin_command_reclaims_expired_processing_lease() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");

    let command_id = Uuid::from_u128(0x715);
    let source_event_id = Uuid::from_u128(0x716);
    let command = CommandEnvelope::new(
        command_id,
        source_event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": "expired-lease" }),
    )
    .unwrap();

    let first = PostgresOrchestrationStore::begin_command(
        &tx,
        "projection-builder",
        &command,
        ts(10),
        ts(310),
    )
    .await
    .expect("first command begin should insert processing lease");
    assert!(matches!(
        first,
        CommandBeginOutcome::FirstSeen(entry)
            if entry.status == musubi_orchestration::CommandInboxStatus::Processing
                && entry.claimed_until == Some(ts(310))
    ));

    let reclaimed = PostgresOrchestrationStore::begin_command(
        &tx,
        "projection-builder",
        &command,
        ts(311),
        ts(611),
    )
    .await
    .expect("expired processing lease should be ready for retry");
    assert!(matches!(
        reclaimed,
        CommandBeginOutcome::ReadyForRetry(entry)
            if entry.status == musubi_orchestration::CommandInboxStatus::Processing
                && entry.claimed_until == Some(ts(611))
    ));

    let row = tx
        .query_one(
            "
            SELECT status, claimed_by, claimed_until
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to read reclaimed command inbox row");
    let status: String = row.get("status");
    let claimed_by: Option<String> = row.get("claimed_by");
    let claimed_until: Option<chrono::DateTime<Utc>> = row.get("claimed_until");

    assert_eq!(status, "processing");
    assert_eq!(claimed_by.as_deref(), Some("projection-builder"));
    assert_eq!(claimed_until, Some(ts(611)));

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn legacy_command_rows_without_payload_checksum_fail_gracefully() {
    let Ok(database_url) = std::env::var("MUSUBI_TEST_DATABASE_URL") else {
        return;
    };

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls)
        .await
        .expect("failed to connect to MUSUBI_TEST_DATABASE_URL");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let tx = client
        .transaction()
        .await
        .expect("failed to open transaction");
    tx.batch_execute(include_str!(
        "../../../migrations/0004_create_outbox_schema.sql"
    ))
    .await
    .expect("failed to apply outbox schema");
    tx.batch_execute(include_str!(
        "../../../migrations/0006_orchestration_runtime_baseline.sql"
    ))
    .await
    .expect("failed to apply orchestration baseline migration");

    let command_id = Uuid::from_u128(0x704);
    let source_event_id = Uuid::from_u128(0x705);
    tx.execute(
        "
        INSERT INTO outbox.command_inbox (
            inbox_entry_id,
            consumer_name,
            command_id,
            source_event_id,
            payload_checksum,
            received_at,
            status,
            available_at,
            attempt_count,
            command_type,
            schema_version
        )
        VALUES ($1, $2, $3, $4, NULL, $5, 'pending', $5, 0, $6, $7)
        ",
        &[
            &Uuid::new_v4(),
            &"projection-builder",
            &command_id,
            &source_event_id,
            &ts(0),
            &"projection.refresh",
            &1_i32,
        ],
    )
    .await
    .expect("failed to insert legacy command row");

    let result = PostgresOrchestrationStore::begin_command(
        &tx,
        "projection-builder",
        &CommandEnvelope::new(
            command_id,
            source_event_id,
            "projection.refresh",
            1,
            json!({ "settlement_case_id": "legacy" }),
        )
        .unwrap(),
        ts(10),
        ts(310),
    )
    .await;

    assert_eq!(
        result,
        Err(OrchestrationError::Database(
            "command inbox row is missing payload_checksum".to_owned()
        ))
    );

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}
