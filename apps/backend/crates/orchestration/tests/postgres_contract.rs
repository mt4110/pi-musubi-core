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
async fn postgres_prune_is_idempotent_with_existing_archive_rows() {
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

    let aggregate_id = Uuid::from_u128(0x727);
    let event_id = Uuid::from_u128(0x728);
    let command_id = Uuid::from_u128(0x729);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x72a),
        stream_key: "settlement_case:archive-conflict".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id,
        event_type: "settlement.submit_action".to_owned(),
        schema_version: 1,
        payload_json: json!({
            "intent_id": "intent-archive-conflict",
            "retention_probe": { "kind": "archive_conflict_idempotency" }
        }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
        .await
        .expect("failed to insert archive conflict outbox message");
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
            &"provider-key-archive-conflict",
        ],
    )
    .await
    .expect("failed to mark archive conflict outbox event terminal");
    let causal_order: i64 = tx
        .query_one(
            "SELECT causal_order FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to read archive conflict causal order")
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
            &"provider-key-archive-conflict",
        ],
    )
    .await
    .expect("failed to insert archive conflict outbox attempt");

    let command = CommandEnvelope::new(
        command_id,
        event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": aggregate_id }),
    )
    .unwrap();
    let result_json = json!({
        "projection_id": "projection-archive-conflict",
        "archive_conflict_seen": true
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
    .expect("failed to insert archive conflict command inbox row");

    tx.execute(
        "
        INSERT INTO outbox.outbox_event_archive (
            event_id,
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
            quarantined_at,
            last_attempt_at,
            last_error_class,
            last_error_code,
            last_error_detail,
            quarantine_reason,
            retain_until,
            published_external_idempotency_key
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, 1, $7, $8, 'published', 1, $9,
            $10, $10, $11, NULL, $11, NULL, NULL, NULL, NULL, $12, $13
        )
        ",
        &[
            &event_id,
            &ts(150),
            &message.stream_key,
            &message.aggregate_type,
            &aggregate_id,
            &message.event_type,
            &message.payload_json,
            &message.payload_hash,
            &causal_order,
            &ts(0),
            &ts(10),
            &ts(100),
            &"provider-key-archive-conflict",
        ],
    )
    .await
    .expect("failed to seed existing outbox event archive row");
    tx.execute(
        "
        INSERT INTO outbox.outbox_attempt_archive (
            event_id,
            attempt_number,
            archived_at,
            relay_name,
            claimed_at,
            claimed_until,
            finished_at,
            failure_class,
            failure_code,
            failure_detail,
            external_idempotency_key
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, NULL, NULL, $8)
        ",
        &[
            &event_id,
            &1_i32,
            &ts(151),
            &"settlement-relay",
            &ts(0),
            &ts(300),
            &ts(10),
            &"provider-key-archive-conflict",
        ],
    )
    .await
    .expect("failed to seed existing outbox attempt archive row");
    tx.execute(
        "
        INSERT INTO outbox.command_inbox_archive (
            consumer_name,
            command_id,
            source_event_id,
            archived_at,
            command_type,
            schema_version,
            status,
            attempt_count,
            payload_checksum,
            received_at,
            processed_at,
            completed_at,
            last_error_class,
            last_error_code,
            last_error_detail,
            quarantine_reason,
            result_type,
            result_json,
            retain_until
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'completed', 1, $7, $8, NULL, $9, NULL, NULL, NULL, NULL, $10, $11, $12)
        ",
        &[
            &"projection-builder",
            &command_id,
            &event_id,
            &ts(152),
            &command.command_type,
            &command.schema_version,
            &command.payload_hash,
            &ts(20),
            &ts(30),
            &"projected",
            &result_json,
            &ts(100),
        ],
    )
    .await
    .expect("failed to seed existing command inbox archive row");

    let first = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune archive conflict coordination rows");

    assert_eq!(first.pruned_outbox_event_ids, vec![event_id]);
    assert_eq!(
        first.pruned_command_keys,
        vec![CommandKey {
            consumer_name: "projection-builder".to_owned(),
            command_id,
        }]
    );

    let second = PostgresOrchestrationStore::prune_coordination(&tx, ts(201))
        .await
        .expect("second archive conflict prune should be idempotent");

    assert!(second.pruned_outbox_event_ids.is_empty());
    assert!(second.pruned_command_keys.is_empty());

    let archived_event = tx
        .query_one(
            "
            SELECT COUNT(*) AS count, MIN(archived_at) AS archived_at
            FROM outbox.outbox_event_archive
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("failed to read archive conflict outbox archive state");
    let archived_attempt = tx
        .query_one(
            "
            SELECT COUNT(*) AS count, MIN(archived_at) AS archived_at
            FROM outbox.outbox_attempt_archive
            WHERE event_id = $1
              AND attempt_number = $2
            ",
            &[&event_id, &1_i32],
        )
        .await
        .expect("failed to read archive conflict attempt archive state");
    let archived_command = tx
        .query_one(
            "
            SELECT COUNT(*) AS count, MIN(archived_at) AS archived_at
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &command_id],
        )
        .await
        .expect("failed to read archive conflict command archive state");

    assert_eq!(archived_event.get::<_, i64>("count"), 1);
    assert_eq!(
        archived_event.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(150)
    );
    assert_eq!(archived_attempt.get::<_, i64>("count"), 1);
    assert_eq!(
        archived_attempt.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(151)
    );
    assert_eq!(archived_command.get::<_, i64>("count"), 1);
    assert_eq!(
        archived_command.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(152)
    );

    let hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox events after archive conflict prune")
        .get("count");
    let hot_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox attempts after archive conflict prune")
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
        .expect("failed to count hot command rows after archive conflict prune")
        .get("count");

    assert_eq!(hot_outbox_count, 0);
    assert_eq!(hot_attempt_count, 0);
    assert_eq!(hot_command_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_preserves_terminal_rows_before_retain_until() {
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

    let published_null_event_id = Uuid::from_u128(0x72b);
    let published_future_event_id = Uuid::from_u128(0x72c);
    let quarantined_null_event_id = Uuid::from_u128(0x72d);
    let quarantined_future_event_id = Uuid::from_u128(0x72e);
    let completed_null_command_id = Uuid::from_u128(0x737);
    let completed_future_command_id = Uuid::from_u128(0x738);
    let quarantined_null_command_id = Uuid::from_u128(0x739);
    let quarantined_future_command_id = Uuid::from_u128(0x73a);

    let outbox_cases: [(Uuid, Uuid, Uuid, &str, &str, Option<chrono::DateTime<Utc>>); 4] = [
        (
            published_null_event_id,
            Uuid::from_u128(0x72f),
            Uuid::from_u128(0x733),
            "published-retain-null",
            "published",
            None,
        ),
        (
            published_future_event_id,
            Uuid::from_u128(0x730),
            Uuid::from_u128(0x734),
            "published-retain-future",
            "published",
            Some(ts(300)),
        ),
        (
            quarantined_null_event_id,
            Uuid::from_u128(0x731),
            Uuid::from_u128(0x735),
            "quarantined-retain-null",
            "quarantined",
            None,
        ),
        (
            quarantined_future_event_id,
            Uuid::from_u128(0x732),
            Uuid::from_u128(0x736),
            "quarantined-retain-future",
            "quarantined",
            Some(ts(300)),
        ),
    ];

    for (event_id, idempotency_key, aggregate_id, label, delivery_status, retain_until) in
        outbox_cases
    {
        let message = NewOutboxMessage::new(NewOutboxMessageSpec {
            event_id,
            idempotency_key,
            stream_key: format!("settlement_case:{label}"),
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            event_type: "settlement.submit_action".to_owned(),
            schema_version: 1,
            payload_json: json!({
                "intent_id": label,
                "retention_probe": { "kind": "terminal_prune_retention_eligibility" }
            }),
            available_at: ts(0),
            created_at: ts(0),
        })
        .unwrap();

        PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
            .await
            .expect("failed to insert retention eligibility outbox message");

        if delivery_status == "published" {
            let external_key = format!("provider-key-{label}");
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
                &[&event_id, &ts(10), &retain_until, &external_key],
            )
            .await
            .expect("failed to mark retention eligibility outbox event published");
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
                    &external_key,
                ],
            )
            .await
            .expect("failed to insert retention eligibility published attempt");
        } else {
            tx.execute(
                "
                UPDATE outbox.events
                SET delivery_status = 'quarantined',
                    attempt_count = 1,
                    quarantined_at = $2,
                    last_attempt_at = $2,
                    last_error_class = $3,
                    last_error_code = $4,
                    last_error_detail = $5,
                    quarantine_reason = $6,
                    retain_until = $7
                WHERE event_id = $1
                ",
                &[
                    &event_id,
                    &ts(10),
                    &"permanent",
                    &"provider_rejected",
                    &"terminal retention eligibility quarantine detail",
                    &"permanent_failure",
                    &retain_until,
                ],
            )
            .await
            .expect("failed to mark retention eligibility outbox event quarantined");
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
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL)
                ",
                &[
                    &event_id,
                    &1_i32,
                    &"settlement-relay",
                    &ts(0),
                    &ts(300),
                    &ts(10),
                    &"permanent",
                    &"provider_rejected",
                    &"terminal retention eligibility attempt detail",
                ],
            )
            .await
            .expect("failed to insert retention eligibility quarantined attempt");
        }
    }

    let command_cases: [(Uuid, Uuid, &str, &str, Option<chrono::DateTime<Utc>>); 4] = [
        (
            completed_null_command_id,
            published_null_event_id,
            "completed-retain-null",
            "completed",
            None,
        ),
        (
            completed_future_command_id,
            published_future_event_id,
            "completed-retain-future",
            "completed",
            Some(ts(300)),
        ),
        (
            quarantined_null_command_id,
            quarantined_null_event_id,
            "quarantined-retain-null",
            "quarantined",
            None,
        ),
        (
            quarantined_future_command_id,
            quarantined_future_event_id,
            "quarantined-retain-future",
            "quarantined",
            Some(ts(300)),
        ),
    ];

    for (command_id, source_event_id, label, status, retain_until) in command_cases {
        let command = CommandEnvelope::new(
            command_id,
            source_event_id,
            "projection.refresh",
            1,
            json!({ "settlement_case_id": label }),
        )
        .unwrap();

        if status == "completed" {
            let result_json = json!({ "projection_id": label });
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
                    &source_event_id,
                    &command.payload_hash,
                    &ts(20),
                    &command.command_type,
                    &command.schema_version,
                    &ts(30),
                    &"projected",
                    &result_json,
                    &retain_until,
                ],
            )
            .await
            .expect("failed to insert retention eligibility completed command inbox row");
        } else {
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
                    processed_at,
                    last_error_class,
                    last_error_code,
                    last_error_detail,
                    quarantine_reason,
                    retain_until
                )
                VALUES ($1, $2, $3, $4, $5, $6, 'quarantined', $6, 1, $7, $8, $9, $10, $11, $12, $13, $14)
                ",
                &[
                    &Uuid::new_v4(),
                    &"projection-builder",
                    &command_id,
                    &source_event_id,
                    &command.payload_hash,
                    &ts(20),
                    &command.command_type,
                    &command.schema_version,
                    &ts(30),
                    &"permanent",
                    &"projection_failed",
                    &"terminal retention eligibility command detail",
                    &"permanent_failure",
                    &retain_until,
                ],
            )
            .await
            .expect("failed to insert retention eligibility quarantined command inbox row");
        }
    }

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune retention eligibility coordination rows");

    assert!(outcome.pruned_outbox_event_ids.is_empty());
    assert!(outcome.pruned_command_keys.is_empty());

    let hot_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.events
            WHERE event_id IN ($1, $2, $3, $4)
            ",
            &[
                &published_null_event_id,
                &published_future_event_id,
                &quarantined_null_event_id,
                &quarantined_future_event_id,
            ],
        )
        .await
        .expect("failed to count retention eligibility hot outbox rows")
        .get("count");
    let archived_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_event_archive
            WHERE event_id IN ($1, $2, $3, $4)
            ",
            &[
                &published_null_event_id,
                &published_future_event_id,
                &quarantined_null_event_id,
                &quarantined_future_event_id,
            ],
        )
        .await
        .expect("failed to count retention eligibility archived outbox rows")
        .get("count");
    let hot_attempt_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_attempts
            WHERE event_id IN ($1, $2, $3, $4)
            ",
            &[
                &published_null_event_id,
                &published_future_event_id,
                &quarantined_null_event_id,
                &quarantined_future_event_id,
            ],
        )
        .await
        .expect("failed to count retention eligibility hot attempt rows")
        .get("count");
    let archived_attempt_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_attempt_archive
            WHERE event_id IN ($1, $2, $3, $4)
            ",
            &[
                &published_null_event_id,
                &published_future_event_id,
                &quarantined_null_event_id,
                &quarantined_future_event_id,
            ],
        )
        .await
        .expect("failed to count retention eligibility archived attempt rows")
        .get("count");
    let hot_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id IN ($2, $3, $4, $5)
            ",
            &[
                &"projection-builder",
                &completed_null_command_id,
                &completed_future_command_id,
                &quarantined_null_command_id,
                &quarantined_future_command_id,
            ],
        )
        .await
        .expect("failed to count retention eligibility hot command inbox rows")
        .get("count");
    let archived_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id IN ($2, $3, $4, $5)
            ",
            &[
                &"projection-builder",
                &completed_null_command_id,
                &completed_future_command_id,
                &quarantined_null_command_id,
                &quarantined_future_command_id,
            ],
        )
        .await
        .expect("failed to count retention eligibility archived command inbox rows")
        .get("count");

    assert_eq!(hot_outbox_count, 4);
    assert_eq!(archived_outbox_count, 0);
    assert_eq!(hot_attempt_count, 4);
    assert_eq!(archived_attempt_count, 0);
    assert_eq!(hot_command_count, 4);
    assert_eq!(archived_command_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_separates_mixed_retention_eligibility_rows() {
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

    let eligible_event_id = Uuid::from_u128(0x73b);
    let retained_null_event_id = Uuid::from_u128(0x73c);
    let retained_future_event_id = Uuid::from_u128(0x73d);
    let nonterminal_event_id = Uuid::from_u128(0x73e);
    let eligible_command_id = Uuid::from_u128(0x747);
    let retained_null_command_id = Uuid::from_u128(0x748);
    let retained_future_command_id = Uuid::from_u128(0x749);
    let nonterminal_command_id = Uuid::from_u128(0x74a);

    let outbox_cases: [(Uuid, Uuid, Uuid, &str, &str, Option<chrono::DateTime<Utc>>); 3] = [
        (
            eligible_event_id,
            Uuid::from_u128(0x73f),
            Uuid::from_u128(0x742),
            "mixed-eligible-published",
            "published",
            Some(ts(100)),
        ),
        (
            retained_null_event_id,
            Uuid::from_u128(0x740),
            Uuid::from_u128(0x743),
            "mixed-retained-null",
            "published",
            None,
        ),
        (
            retained_future_event_id,
            Uuid::from_u128(0x741),
            Uuid::from_u128(0x744),
            "mixed-retained-future",
            "quarantined",
            Some(ts(300)),
        ),
    ];

    for (event_id, idempotency_key, aggregate_id, label, delivery_status, retain_until) in
        outbox_cases
    {
        let message = NewOutboxMessage::new(NewOutboxMessageSpec {
            event_id,
            idempotency_key,
            stream_key: format!("settlement_case:{label}"),
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            event_type: "settlement.submit_action".to_owned(),
            schema_version: 1,
            payload_json: json!({
                "intent_id": label,
                "retention_probe": { "kind": "mixed_retention_eligibility" }
            }),
            available_at: ts(0),
            created_at: ts(0),
        })
        .unwrap();

        PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
            .await
            .expect("failed to insert mixed eligibility outbox message");

        if delivery_status == "published" {
            let external_key = format!("provider-key-{label}");
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
                &[&event_id, &ts(10), &retain_until, &external_key],
            )
            .await
            .expect("failed to mark mixed eligibility outbox event published");
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
                    &external_key,
                ],
            )
            .await
            .expect("failed to insert mixed eligibility published attempt");
        } else {
            tx.execute(
                "
                UPDATE outbox.events
                SET delivery_status = 'quarantined',
                    attempt_count = 1,
                    quarantined_at = $2,
                    last_attempt_at = $2,
                    last_error_class = $3,
                    last_error_code = $4,
                    last_error_detail = $5,
                    quarantine_reason = $6,
                    retain_until = $7
                WHERE event_id = $1
                ",
                &[
                    &event_id,
                    &ts(10),
                    &"permanent",
                    &"provider_rejected",
                    &"mixed eligibility quarantine detail",
                    &"permanent_failure",
                    &retain_until,
                ],
            )
            .await
            .expect("failed to mark mixed eligibility outbox event quarantined");
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
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL)
                ",
                &[
                    &event_id,
                    &1_i32,
                    &"settlement-relay",
                    &ts(0),
                    &ts(300),
                    &ts(10),
                    &"permanent",
                    &"provider_rejected",
                    &"mixed eligibility attempt detail",
                ],
            )
            .await
            .expect("failed to insert mixed eligibility quarantined attempt");
        }
    }

    let nonterminal_message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id: nonterminal_event_id,
        idempotency_key: Uuid::from_u128(0x745),
        stream_key: "settlement_case:mixed-nonterminal".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id: Uuid::from_u128(0x746),
        event_type: "settlement.submit_action".to_owned(),
        schema_version: 1,
        payload_json: json!({
            "intent_id": "mixed-nonterminal",
            "retention_probe": { "kind": "mixed_retention_eligibility_nonterminal" }
        }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();
    PostgresOrchestrationStore::insert_outbox_message(&tx, &nonterminal_message)
        .await
        .expect("failed to insert mixed eligibility nonterminal outbox message");
    tx.execute(
        "
        UPDATE outbox.events
        SET retain_until = $2
        WHERE event_id = $1
        ",
        &[&nonterminal_event_id, &ts(100)],
    )
    .await
    .expect("failed to mark mixed eligibility nonterminal outbox retain_until");

    let command_cases: [(Uuid, Uuid, &str, &str, Option<chrono::DateTime<Utc>>); 3] = [
        (
            eligible_command_id,
            eligible_event_id,
            "mixed-eligible-command",
            "completed",
            Some(ts(100)),
        ),
        (
            retained_null_command_id,
            retained_null_event_id,
            "mixed-retained-null-command",
            "completed",
            None,
        ),
        (
            retained_future_command_id,
            retained_future_event_id,
            "mixed-retained-future-command",
            "quarantined",
            Some(ts(300)),
        ),
    ];

    for (command_id, source_event_id, label, status, retain_until) in command_cases {
        let command = CommandEnvelope::new(
            command_id,
            source_event_id,
            "projection.refresh",
            1,
            json!({ "settlement_case_id": label }),
        )
        .unwrap();

        if status == "completed" {
            let result_json = json!({ "projection_id": label });
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
                    &source_event_id,
                    &command.payload_hash,
                    &ts(20),
                    &command.command_type,
                    &command.schema_version,
                    &ts(30),
                    &"projected",
                    &result_json,
                    &retain_until,
                ],
            )
            .await
            .expect("failed to insert mixed eligibility completed command inbox row");
        } else {
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
                    processed_at,
                    last_error_class,
                    last_error_code,
                    last_error_detail,
                    quarantine_reason,
                    retain_until
                )
                VALUES ($1, $2, $3, $4, $5, $6, 'quarantined', $6, 1, $7, $8, $9, $10, $11, $12, $13, $14)
                ",
                &[
                    &Uuid::new_v4(),
                    &"projection-builder",
                    &command_id,
                    &source_event_id,
                    &command.payload_hash,
                    &ts(20),
                    &command.command_type,
                    &command.schema_version,
                    &ts(30),
                    &"permanent",
                    &"projection_failed",
                    &"mixed eligibility command detail",
                    &"permanent_failure",
                    &retain_until,
                ],
            )
            .await
            .expect("failed to insert mixed eligibility quarantined command inbox row");
        }
    }

    let nonterminal_command = CommandEnvelope::new(
        nonterminal_command_id,
        nonterminal_event_id,
        "projection.refresh",
        1,
        json!({ "settlement_case_id": "mixed-nonterminal" }),
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
            claimed_by,
            claimed_until,
            retain_until
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'processing', $6, 0, $7, $8, $9, $10, $11)
        ",
        &[
            &Uuid::new_v4(),
            &"projection-builder",
            &nonterminal_command.command_id,
            &nonterminal_command.source_event_id,
            &nonterminal_command.payload_hash,
            &ts(0),
            &nonterminal_command.command_type,
            &nonterminal_command.schema_version,
            &"projection-builder",
            &ts(300),
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert mixed eligibility nonterminal command inbox row");

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune mixed eligibility coordination rows");

    assert_eq!(outcome.pruned_outbox_event_ids, vec![eligible_event_id]);
    assert_eq!(
        outcome.pruned_command_keys,
        vec![CommandKey {
            consumer_name: "projection-builder".to_owned(),
            command_id: eligible_command_id,
        }]
    );

    let hot_eligible_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&eligible_event_id],
        )
        .await
        .expect("failed to count mixed eligibility hot eligible outbox row")
        .get("count");
    let archived_eligible_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_event_archive WHERE event_id = $1",
            &[&eligible_event_id],
        )
        .await
        .expect("failed to count mixed eligibility archived eligible outbox row")
        .get("count");
    let hot_eligible_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&eligible_event_id],
        )
        .await
        .expect("failed to count mixed eligibility hot eligible attempt row")
        .get("count");
    let archived_eligible_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempt_archive WHERE event_id = $1",
            &[&eligible_event_id],
        )
        .await
        .expect("failed to count mixed eligibility archived eligible attempt row")
        .get("count");
    let hot_eligible_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &eligible_command_id],
        )
        .await
        .expect("failed to count mixed eligibility hot eligible command row")
        .get("count");
    let archived_eligible_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &eligible_command_id],
        )
        .await
        .expect("failed to count mixed eligibility archived eligible command row")
        .get("count");

    let hot_retained_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.events
            WHERE event_id IN ($1, $2)
            ",
            &[&retained_null_event_id, &retained_future_event_id],
        )
        .await
        .expect("failed to count mixed eligibility retained hot outbox rows")
        .get("count");
    let archived_retained_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_event_archive
            WHERE event_id IN ($1, $2)
            ",
            &[&retained_null_event_id, &retained_future_event_id],
        )
        .await
        .expect("failed to count mixed eligibility retained archived outbox rows")
        .get("count");
    let hot_retained_attempt_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_attempts
            WHERE event_id IN ($1, $2)
            ",
            &[&retained_null_event_id, &retained_future_event_id],
        )
        .await
        .expect("failed to count mixed eligibility retained hot attempt rows")
        .get("count");
    let archived_retained_attempt_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_attempt_archive
            WHERE event_id IN ($1, $2)
            ",
            &[&retained_null_event_id, &retained_future_event_id],
        )
        .await
        .expect("failed to count mixed eligibility retained archived attempt rows")
        .get("count");
    let hot_retained_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id IN ($2, $3)
            ",
            &[
                &"projection-builder",
                &retained_null_command_id,
                &retained_future_command_id,
            ],
        )
        .await
        .expect("failed to count mixed eligibility retained hot command rows")
        .get("count");
    let archived_retained_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id IN ($2, $3)
            ",
            &[
                &"projection-builder",
                &retained_null_command_id,
                &retained_future_command_id,
            ],
        )
        .await
        .expect("failed to count mixed eligibility retained archived command rows")
        .get("count");

    let hot_nonterminal_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&nonterminal_event_id],
        )
        .await
        .expect("failed to count mixed eligibility nonterminal hot outbox row")
        .get("count");
    let archived_nonterminal_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_event_archive WHERE event_id = $1",
            &[&nonterminal_event_id],
        )
        .await
        .expect("failed to count mixed eligibility nonterminal archived outbox row")
        .get("count");
    let hot_nonterminal_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &nonterminal_command_id],
        )
        .await
        .expect("failed to count mixed eligibility nonterminal hot command row")
        .get("count");
    let archived_nonterminal_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &nonterminal_command_id],
        )
        .await
        .expect("failed to count mixed eligibility nonterminal archived command row")
        .get("count");

    assert_eq!(hot_eligible_outbox_count, 0);
    assert_eq!(archived_eligible_outbox_count, 1);
    assert_eq!(hot_eligible_attempt_count, 0);
    assert_eq!(archived_eligible_attempt_count, 1);
    assert_eq!(hot_eligible_command_count, 0);
    assert_eq!(archived_eligible_command_count, 1);
    assert_eq!(hot_retained_outbox_count, 2);
    assert_eq!(archived_retained_outbox_count, 0);
    assert_eq!(hot_retained_attempt_count, 2);
    assert_eq!(archived_retained_attempt_count, 0);
    assert_eq!(hot_retained_command_count, 2);
    assert_eq!(archived_retained_command_count, 0);
    assert_eq!(hot_nonterminal_outbox_count, 1);
    assert_eq!(archived_nonterminal_outbox_count, 0);
    assert_eq!(hot_nonterminal_command_count, 1);
    assert_eq!(archived_nonterminal_command_count, 0);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_returns_deterministic_outcome_ordering() {
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

    let late_event_id = Uuid::from_u128(0x760);
    let first_event_id = Uuid::from_u128(0x761);
    let middle_event_id = Uuid::from_u128(0x762);
    let retained_event_id = Uuid::from_u128(0x763);
    let nonterminal_event_id = Uuid::from_u128(0x764);

    let outbox_cases: [(Uuid, Uuid, Uuid, &str, &str, i64, chrono::DateTime<Utc>); 5] = [
        (
            late_event_id,
            Uuid::from_u128(0x765),
            Uuid::from_u128(0x766),
            "ordering-late",
            "published",
            30,
            ts(100),
        ),
        (
            first_event_id,
            Uuid::from_u128(0x767),
            Uuid::from_u128(0x768),
            "ordering-first",
            "published",
            10,
            ts(100),
        ),
        (
            middle_event_id,
            Uuid::from_u128(0x769),
            Uuid::from_u128(0x76a),
            "ordering-middle",
            "published",
            20,
            ts(100),
        ),
        (
            retained_event_id,
            Uuid::from_u128(0x76b),
            Uuid::from_u128(0x76c),
            "ordering-retained",
            "published",
            5,
            ts(300),
        ),
        (
            nonterminal_event_id,
            Uuid::from_u128(0x76d),
            Uuid::from_u128(0x76e),
            "ordering-nonterminal",
            "pending",
            15,
            ts(100),
        ),
    ];

    for (
        event_id,
        idempotency_key,
        aggregate_id,
        label,
        delivery_status,
        causal_order,
        retain_until,
    ) in outbox_cases
    {
        let message = NewOutboxMessage::new(NewOutboxMessageSpec {
            event_id,
            idempotency_key,
            stream_key: format!("settlement_case:{label}"),
            aggregate_type: "settlement_case".to_owned(),
            aggregate_id,
            event_type: "settlement.submit_action".to_owned(),
            schema_version: 1,
            payload_json: json!({
                "intent_id": label,
                "retention_probe": { "kind": "deterministic_outcome_ordering" }
            }),
            available_at: ts(0),
            created_at: ts(0),
        })
        .unwrap();
        let attempt_count = if delivery_status == "published" {
            1_i32
        } else {
            0_i32
        };
        let published_at = if delivery_status == "published" {
            Some(ts(10))
        } else {
            None
        };
        let external_key = if delivery_status == "published" {
            Some(format!("provider-key-{label}"))
        } else {
            None
        };

        tx.execute(
            "
            INSERT INTO outbox.events (
                event_id,
                idempotency_key,
                stream_key,
                aggregate_type,
                aggregate_id,
                event_type,
                schema_version,
                payload_json,
                payload_hash,
                delivery_status,
                attempt_count,
                causal_order,
                available_at,
                created_at,
                published_at,
                last_attempt_at,
                retain_until,
                published_external_idempotency_key
            )
            OVERRIDING SYSTEM VALUE
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15, $16, $17)
            ",
            &[
                &message.event_id,
                &message.idempotency_key,
                &message.stream_key,
                &message.aggregate_type,
                &message.aggregate_id,
                &message.event_type,
                &message.schema_version,
                &message.payload_json,
                &message.payload_hash,
                &delivery_status,
                &attempt_count,
                &causal_order,
                &message.available_at,
                &message.created_at,
                &published_at,
                &retain_until,
                &external_key,
            ],
        )
        .await
        .expect("failed to insert deterministic ordering outbox event");

        if delivery_status == "published" {
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
                    &external_key,
                ],
            )
            .await
            .expect("failed to insert deterministic ordering outbox attempt");
        }
    }

    let zeta_command_id = Uuid::from_u128(0x773);
    let alpha_high_command_id = Uuid::from_u128(0x772);
    let alpha_low_command_id = Uuid::from_u128(0x771);
    let retained_command_id = Uuid::from_u128(0x774);
    let nonterminal_command_id = Uuid::from_u128(0x775);

    let command_cases: [(
        &str,
        Uuid,
        Uuid,
        &str,
        &str,
        chrono::DateTime<Utc>,
        chrono::DateTime<Utc>,
    ); 5] = [
        (
            "projection-zeta",
            zeta_command_id,
            late_event_id,
            "ordering-zeta",
            "completed",
            ts(100),
            ts(30),
        ),
        (
            "projection-alpha",
            alpha_high_command_id,
            middle_event_id,
            "ordering-alpha-high",
            "completed",
            ts(100),
            ts(10),
        ),
        (
            "projection-alpha",
            alpha_low_command_id,
            first_event_id,
            "ordering-alpha-low",
            "completed",
            ts(100),
            ts(20),
        ),
        (
            "projection-alpha",
            retained_command_id,
            retained_event_id,
            "ordering-retained-command",
            "completed",
            ts(300),
            ts(0),
        ),
        (
            "projection-alpha",
            nonterminal_command_id,
            nonterminal_event_id,
            "ordering-nonterminal-command",
            "processing",
            ts(100),
            ts(40),
        ),
    ];

    for (consumer_name, command_id, source_event_id, label, status, retain_until, received_at) in
        command_cases
    {
        let command = CommandEnvelope::new(
            command_id,
            source_event_id,
            "projection.refresh",
            1,
            json!({ "settlement_case_id": label }),
        )
        .unwrap();

        if status == "completed" {
            let result_json = json!({ "projection_id": label });
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
                    &consumer_name,
                    &command.command_id,
                    &command.source_event_id,
                    &command.payload_hash,
                    &received_at,
                    &command.command_type,
                    &command.schema_version,
                    &ts(50),
                    &"projected",
                    &result_json,
                    &retain_until,
                ],
            )
            .await
            .expect("failed to insert deterministic ordering completed command inbox row");
        } else {
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
                VALUES ($1, $2, $3, $4, $5, $6, 'processing', $6, 0, $7, $8, $9, $10, $11)
                ",
                &[
                    &Uuid::new_v4(),
                    &consumer_name,
                    &command.command_id,
                    &command.source_event_id,
                    &command.payload_hash,
                    &received_at,
                    &command.command_type,
                    &command.schema_version,
                    &"projection-builder",
                    &ts(300),
                    &retain_until,
                ],
            )
            .await
            .expect("failed to insert deterministic ordering processing command inbox row");
        }
    }

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune deterministic ordering coordination rows");

    assert_eq!(
        outcome.pruned_outbox_event_ids,
        vec![first_event_id, middle_event_id, late_event_id]
    );
    assert_eq!(
        outcome.pruned_command_keys,
        vec![
            CommandKey {
                consumer_name: "projection-alpha".to_owned(),
                command_id: alpha_low_command_id,
            },
            CommandKey {
                consumer_name: "projection-alpha".to_owned(),
                command_id: alpha_high_command_id,
            },
            CommandKey {
                consumer_name: "projection-zeta".to_owned(),
                command_id: zeta_command_id,
            },
        ]
    );

    let hot_eligible_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.events
            WHERE event_id IN ($1, $2, $3)
            ",
            &[&late_event_id, &first_event_id, &middle_event_id],
        )
        .await
        .expect("failed to count deterministic ordering hot eligible outbox rows")
        .get("count");
    let archived_eligible_outbox_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_event_archive
            WHERE event_id IN ($1, $2, $3)
            ",
            &[&late_event_id, &first_event_id, &middle_event_id],
        )
        .await
        .expect("failed to count deterministic ordering archived eligible outbox rows")
        .get("count");
    let hot_eligible_attempt_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_attempts
            WHERE event_id IN ($1, $2, $3)
            ",
            &[&late_event_id, &first_event_id, &middle_event_id],
        )
        .await
        .expect("failed to count deterministic ordering hot eligible attempts")
        .get("count");
    let archived_eligible_attempt_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.outbox_attempt_archive
            WHERE event_id IN ($1, $2, $3)
            ",
            &[&late_event_id, &first_event_id, &middle_event_id],
        )
        .await
        .expect("failed to count deterministic ordering archived eligible attempts")
        .get("count");
    let hot_eligible_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE (consumer_name = $1 AND command_id IN ($2, $3))
               OR (consumer_name = $4 AND command_id = $5)
            ",
            &[
                &"projection-alpha",
                &alpha_low_command_id,
                &alpha_high_command_id,
                &"projection-zeta",
                &zeta_command_id,
            ],
        )
        .await
        .expect("failed to count deterministic ordering hot eligible command rows")
        .get("count");
    let archived_eligible_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE (consumer_name = $1 AND command_id IN ($2, $3))
               OR (consumer_name = $4 AND command_id = $5)
            ",
            &[
                &"projection-alpha",
                &alpha_low_command_id,
                &alpha_high_command_id,
                &"projection-zeta",
                &zeta_command_id,
            ],
        )
        .await
        .expect("failed to count deterministic ordering archived eligible command rows")
        .get("count");
    let retained_hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&retained_event_id],
        )
        .await
        .expect("failed to count deterministic ordering retained hot outbox row")
        .get("count");
    let retained_hot_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&retained_event_id],
        )
        .await
        .expect("failed to count deterministic ordering retained hot attempt")
        .get("count");
    let retained_hot_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-alpha", &retained_command_id],
        )
        .await
        .expect("failed to count deterministic ordering retained hot command row")
        .get("count");
    let nonterminal_hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&nonterminal_event_id],
        )
        .await
        .expect("failed to count deterministic ordering nonterminal hot outbox row")
        .get("count");
    let nonterminal_hot_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-alpha", &nonterminal_command_id],
        )
        .await
        .expect("failed to count deterministic ordering nonterminal hot command row")
        .get("count");

    assert_eq!(hot_eligible_outbox_count, 0);
    assert_eq!(archived_eligible_outbox_count, 3);
    assert_eq!(hot_eligible_attempt_count, 0);
    assert_eq!(archived_eligible_attempt_count, 3);
    assert_eq!(hot_eligible_command_count, 0);
    assert_eq!(archived_eligible_command_count, 3);
    assert_eq!(retained_hot_outbox_count, 1);
    assert_eq!(retained_hot_attempt_count, 1);
    assert_eq!(retained_hot_command_count, 1);
    assert_eq!(nonterminal_hot_outbox_count, 1);
    assert_eq!(nonterminal_hot_command_count, 1);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_archives_all_attempts_for_eligible_terminal_outbox_event() {
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

    let aggregate_id = Uuid::from_u128(0x780);
    let event_id = Uuid::from_u128(0x781);
    let command_id = Uuid::from_u128(0x782);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x783),
        stream_key: "settlement_case:attempt-archive-completeness".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id,
        event_type: "settlement.submit_action".to_owned(),
        schema_version: 1,
        payload_json: json!({
            "intent_id": "intent-attempt-archive-completeness",
            "retention_probe": { "kind": "outbox_attempt_archive_completeness" }
        }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
        .await
        .expect("failed to insert attempt archive completeness outbox message");
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = 'published',
            attempt_count = 3,
            published_at = $2,
            last_attempt_at = $3,
            retain_until = $4,
            published_external_idempotency_key = $5
        WHERE event_id = $1
        ",
        &[
            &event_id,
            &ts(30),
            &ts(30),
            &ts(100),
            &"provider-key-attempt-completeness-3",
        ],
    )
    .await
    .expect("failed to mark attempt archive completeness outbox event terminal");

    let attempts: [(
        i32,
        &str,
        chrono::DateTime<Utc>,
        chrono::DateTime<Utc>,
        chrono::DateTime<Utc>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
        Option<&str>,
    ); 3] = [
        (
            1,
            "settlement-relay-a",
            ts(0),
            ts(300),
            ts(10),
            Some("transient"),
            Some("rate_limited"),
            Some("relay rate limit"),
            Some("provider-key-attempt-completeness-1"),
        ),
        (
            2,
            "settlement-relay-b",
            ts(11),
            ts(311),
            ts(20),
            Some("transient"),
            Some("timeout"),
            Some("provider timeout"),
            Some("provider-key-attempt-completeness-2"),
        ),
        (
            3,
            "settlement-relay-a",
            ts(21),
            ts(321),
            ts(30),
            None,
            None,
            None,
            Some("provider-key-attempt-completeness-3"),
        ),
    ];

    for (
        attempt_number,
        relay_name,
        claimed_at,
        claimed_until,
        finished_at,
        failure_class,
        failure_code,
        failure_detail,
        external_idempotency_key,
    ) in &attempts
    {
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
                attempt_number,
                relay_name,
                claimed_at,
                claimed_until,
                finished_at,
                failure_class,
                failure_code,
                failure_detail,
                external_idempotency_key,
            ],
        )
        .await
        .expect("failed to insert attempt archive completeness outbox attempt");
    }

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
            &command.command_id,
            &command.source_event_id,
            &command.payload_hash,
            &ts(40),
            &command.command_type,
            &command.schema_version,
            &ts(50),
            &"projected",
            &json!({ "projection_id": "projection-attempt-archive-completeness" }),
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert attempt archive completeness command inbox row");

    let hot_attempt_count_before: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox attempts before prune")
        .get("count");
    assert_eq!(hot_attempt_count_before, 3);

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune attempt archive completeness coordination rows");

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
            SELECT COUNT(*) OVER () AS archive_row_count,
                   attempt_count
            FROM outbox.outbox_event_archive
            WHERE event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("failed to read attempt archive completeness outbox archive row");
    assert_eq!(archived_event.get::<_, i64>("archive_row_count"), 1);
    assert_eq!(archived_event.get::<_, i32>("attempt_count"), 3);

    let archived_attempts = tx
        .query(
            "
            SELECT
                attempt_number,
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
            ORDER BY attempt_number
            ",
            &[&event_id],
        )
        .await
        .expect("failed to read attempt archive completeness outbox attempt archive rows");
    assert_eq!(archived_attempts.len(), attempts.len());

    for (
        archived_attempt,
        (
            attempt_number,
            relay_name,
            claimed_at,
            claimed_until,
            finished_at,
            failure_class,
            failure_code,
            failure_detail,
            external_idempotency_key,
        ),
    ) in archived_attempts.iter().zip(attempts.iter())
    {
        assert_eq!(
            archived_attempt.get::<_, i32>("attempt_number"),
            *attempt_number
        );
        assert_eq!(archived_attempt.get::<_, String>("relay_name"), *relay_name);
        assert_eq!(
            archived_attempt.get::<_, chrono::DateTime<Utc>>("claimed_at"),
            *claimed_at
        );
        assert_eq!(
            archived_attempt.get::<_, chrono::DateTime<Utc>>("claimed_until"),
            *claimed_until
        );
        assert_eq!(
            archived_attempt.get::<_, chrono::DateTime<Utc>>("finished_at"),
            *finished_at
        );
        assert_eq!(
            archived_attempt
                .get::<_, Option<String>>("failure_class")
                .as_deref(),
            *failure_class
        );
        assert_eq!(
            archived_attempt
                .get::<_, Option<String>>("failure_code")
                .as_deref(),
            *failure_code
        );
        assert_eq!(
            archived_attempt
                .get::<_, Option<String>>("failure_detail")
                .as_deref(),
            *failure_detail
        );
        assert_eq!(
            archived_attempt
                .get::<_, Option<String>>("external_idempotency_key")
                .as_deref(),
            *external_idempotency_key
        );
    }

    let hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox events after attempt archive completeness prune")
        .get("count");
    let hot_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempts WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox attempts after attempt archive completeness prune")
        .get("count");
    let archived_attempt_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_attempt_archive WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count archived outbox attempts after attempt archive completeness prune")
        .get("count");

    assert_eq!(hot_outbox_count, 0);
    assert_eq!(hot_attempt_count, 0);
    assert_eq!(archived_attempt_count, hot_attempt_count_before);

    tx.rollback()
        .await
        .expect("rollback should clean up transactional test state");
}

#[tokio::test]
async fn postgres_prune_archives_all_eligible_command_inbox_rows_for_source_event() {
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

    let aggregate_id = Uuid::from_u128(0x790);
    let event_id = Uuid::from_u128(0x791);
    let completed_projection_command_id = Uuid::from_u128(0x792);
    let completed_audit_command_id = Uuid::from_u128(0x793);
    let quarantined_command_id = Uuid::from_u128(0x794);
    let retained_terminal_command_id = Uuid::from_u128(0x795);
    let nonterminal_command_id = Uuid::from_u128(0x796);
    let message = NewOutboxMessage::new(NewOutboxMessageSpec {
        event_id,
        idempotency_key: Uuid::from_u128(0x797),
        stream_key: "settlement_case:command-inbox-archive-completeness".to_owned(),
        aggregate_type: "settlement_case".to_owned(),
        aggregate_id,
        event_type: "settlement.submit_action".to_owned(),
        schema_version: 1,
        payload_json: json!({
            "intent_id": "intent-command-inbox-archive-completeness",
            "retention_probe": { "kind": "command_inbox_archive_completeness" }
        }),
        available_at: ts(0),
        created_at: ts(0),
    })
    .unwrap();

    PostgresOrchestrationStore::insert_outbox_message(&tx, &message)
        .await
        .expect("failed to insert command inbox archive completeness outbox message");
    tx.execute(
        "
        UPDATE outbox.events
        SET delivery_status = 'published',
            attempt_count = 1,
            published_at = $2,
            last_attempt_at = $3,
            retain_until = $4,
            published_external_idempotency_key = $5
        WHERE event_id = $1
        ",
        &[
            &event_id,
            &ts(30),
            &ts(30),
            &ts(100),
            &"provider-key-command-inbox-completeness",
        ],
    )
    .await
    .expect("failed to mark command inbox archive completeness outbox event terminal");
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
        VALUES ($1, 1, $2, $3, $4, $5, NULL, NULL, NULL, $6)
        ",
        &[
            &event_id,
            &"settlement-relay",
            &ts(0),
            &ts(300),
            &ts(30),
            &"provider-key-command-inbox-completeness",
        ],
    )
    .await
    .expect("failed to insert command inbox archive completeness outbox attempt");

    let completed_projection_command = CommandEnvelope::new(
        completed_projection_command_id,
        event_id,
        "projection.refresh",
        1,
        json!({
            "settlement_case_id": aggregate_id,
            "command_label": "projection-completed"
        }),
    )
    .unwrap();
    let completed_audit_command = CommandEnvelope::new(
        completed_audit_command_id,
        event_id,
        "projection.audit",
        1,
        json!({
            "settlement_case_id": aggregate_id,
            "command_label": "audit-completed"
        }),
    )
    .unwrap();
    let quarantined_command = CommandEnvelope::new(
        quarantined_command_id,
        event_id,
        "projection.refresh",
        1,
        json!({
            "settlement_case_id": aggregate_id,
            "command_label": "search-quarantined"
        }),
    )
    .unwrap();
    let retained_terminal_command = CommandEnvelope::new(
        retained_terminal_command_id,
        event_id,
        "projection.refresh",
        1,
        json!({
            "settlement_case_id": aggregate_id,
            "command_label": "future-retained"
        }),
    )
    .unwrap();
    let nonterminal_command = CommandEnvelope::new(
        nonterminal_command_id,
        event_id,
        "projection.audit",
        1,
        json!({
            "settlement_case_id": aggregate_id,
            "command_label": "processing-retained"
        }),
    )
    .unwrap();

    let completed_projection_result = json!({
        "projection_id": "projection-command-inbox-completeness"
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
            &completed_projection_command.command_id,
            &completed_projection_command.source_event_id,
            &completed_projection_command.payload_hash,
            &ts(40),
            &completed_projection_command.command_type,
            &completed_projection_command.schema_version,
            &ts(50),
            &"projected",
            &completed_projection_result,
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert eligible completed projection command inbox row");

    let completed_audit_result = json!({
        "audit_id": "audit-command-inbox-completeness"
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
            &"audit-projector",
            &completed_audit_command.command_id,
            &completed_audit_command.source_event_id,
            &completed_audit_command.payload_hash,
            &ts(42),
            &completed_audit_command.command_type,
            &completed_audit_command.schema_version,
            &ts(52),
            &"audited",
            &completed_audit_result,
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert eligible completed audit command inbox row");

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
            &"search-indexer",
            &quarantined_command.command_id,
            &quarantined_command.source_event_id,
            &quarantined_command.payload_hash,
            &ts(44),
            &quarantined_command.command_type,
            &quarantined_command.schema_version,
            &"permanent",
            &"poison_command",
            &"command payload rejected by projection worker",
            &"poison_pill",
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert eligible quarantined command inbox row");

    let retained_terminal_result = json!({
        "projection_id": "future-retained-command-inbox-completeness"
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
            &retained_terminal_command.command_id,
            &retained_terminal_command.source_event_id,
            &retained_terminal_command.payload_hash,
            &ts(46),
            &retained_terminal_command.command_type,
            &retained_terminal_command.schema_version,
            &ts(56),
            &"projected",
            &retained_terminal_result,
            &ts(300),
        ],
    )
    .await
    .expect("failed to insert retained terminal command inbox row");

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
        VALUES ($1, $2, $3, $4, $5, $6, 'processing', $6, 0, $7, $8, $9, $10, $11)
        ",
        &[
            &Uuid::new_v4(),
            &"audit-projector",
            &nonterminal_command.command_id,
            &nonterminal_command.source_event_id,
            &nonterminal_command.payload_hash,
            &ts(48),
            &nonterminal_command.command_type,
            &nonterminal_command.schema_version,
            &"audit-projector",
            &ts(360),
            &ts(100),
        ],
    )
    .await
    .expect("failed to insert retained nonterminal command inbox row");

    let hot_command_count_before: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE source_event_id = $1
            ",
            &[&event_id],
        )
        .await
        .expect("failed to count hot command inbox rows before prune")
        .get("count");
    assert_eq!(hot_command_count_before, 5);

    let outcome = PostgresOrchestrationStore::prune_coordination(&tx, ts(200))
        .await
        .expect("failed to prune command inbox archive completeness coordination rows");

    assert_eq!(outcome.pruned_outbox_event_ids, vec![event_id]);
    assert_eq!(
        outcome.pruned_command_keys,
        vec![
            CommandKey {
                consumer_name: "audit-projector".to_owned(),
                command_id: completed_audit_command_id,
            },
            CommandKey {
                consumer_name: "projection-builder".to_owned(),
                command_id: completed_projection_command_id,
            },
            CommandKey {
                consumer_name: "search-indexer".to_owned(),
                command_id: quarantined_command_id,
            },
        ]
    );

    let archived_commands = tx
        .query(
            "
            SELECT
                consumer_name,
                command_id,
                source_event_id,
                archived_at,
                command_type,
                schema_version,
                status,
                attempt_count,
                payload_checksum,
                received_at,
                completed_at,
                last_error_class,
                last_error_code,
                last_error_detail,
                quarantine_reason,
                result_type,
                result_json,
                retain_until
            FROM outbox.command_inbox_archive
            WHERE source_event_id = $1
            ORDER BY consumer_name, command_id
            ",
            &[&event_id],
        )
        .await
        .expect("failed to read command inbox archive completeness rows");
    assert_eq!(archived_commands.len(), 3);

    let archived_audit_command = &archived_commands[0];
    assert_eq!(
        archived_audit_command.get::<_, String>("consumer_name"),
        "audit-projector"
    );
    assert_eq!(
        archived_audit_command.get::<_, Uuid>("command_id"),
        completed_audit_command_id
    );
    assert_eq!(
        archived_audit_command.get::<_, Uuid>("source_event_id"),
        event_id
    );
    assert_eq!(
        archived_audit_command.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_audit_command.get::<_, String>("command_type"),
        completed_audit_command.command_type.as_str()
    );
    assert_eq!(
        archived_audit_command.get::<_, i32>("schema_version"),
        completed_audit_command.schema_version
    );
    assert_eq!(
        archived_audit_command.get::<_, String>("status"),
        "completed"
    );
    assert_eq!(archived_audit_command.get::<_, i32>("attempt_count"), 1);
    assert_eq!(
        archived_audit_command
            .get::<_, Option<String>>("payload_checksum")
            .as_deref(),
        Some(completed_audit_command.payload_hash.as_str())
    );
    assert_eq!(
        archived_audit_command.get::<_, chrono::DateTime<Utc>>("received_at"),
        ts(42)
    );
    assert_eq!(
        archived_audit_command.get::<_, Option<chrono::DateTime<Utc>>>("completed_at"),
        Some(ts(52))
    );
    assert_eq!(
        archived_audit_command
            .get::<_, Option<String>>("result_type")
            .as_deref(),
        Some("audited")
    );
    assert_eq!(
        archived_audit_command.get::<_, Option<serde_json::Value>>("result_json"),
        Some(completed_audit_result)
    );
    assert_eq!(
        archived_audit_command.get::<_, Option<chrono::DateTime<Utc>>>("retain_until"),
        Some(ts(100))
    );
    assert_eq!(
        archived_audit_command
            .get::<_, Option<String>>("last_error_class"),
        None
    );

    let archived_projection_command = &archived_commands[1];
    assert_eq!(
        archived_projection_command.get::<_, String>("consumer_name"),
        "projection-builder"
    );
    assert_eq!(
        archived_projection_command.get::<_, Uuid>("command_id"),
        completed_projection_command_id
    );
    assert_eq!(
        archived_projection_command.get::<_, Uuid>("source_event_id"),
        event_id
    );
    assert_eq!(
        archived_projection_command.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_projection_command.get::<_, String>("command_type"),
        completed_projection_command.command_type.as_str()
    );
    assert_eq!(
        archived_projection_command.get::<_, i32>("schema_version"),
        completed_projection_command.schema_version
    );
    assert_eq!(
        archived_projection_command.get::<_, String>("status"),
        "completed"
    );
    assert_eq!(
        archived_projection_command.get::<_, i32>("attempt_count"),
        1
    );
    assert_eq!(
        archived_projection_command
            .get::<_, Option<String>>("payload_checksum")
            .as_deref(),
        Some(completed_projection_command.payload_hash.as_str())
    );
    assert_eq!(
        archived_projection_command.get::<_, chrono::DateTime<Utc>>("received_at"),
        ts(40)
    );
    assert_eq!(
        archived_projection_command.get::<_, Option<chrono::DateTime<Utc>>>("completed_at"),
        Some(ts(50))
    );
    assert_eq!(
        archived_projection_command
            .get::<_, Option<String>>("result_type")
            .as_deref(),
        Some("projected")
    );
    assert_eq!(
        archived_projection_command.get::<_, Option<serde_json::Value>>("result_json"),
        Some(completed_projection_result)
    );
    assert_eq!(
        archived_projection_command.get::<_, Option<chrono::DateTime<Utc>>>("retain_until"),
        Some(ts(100))
    );
    assert_eq!(
        archived_projection_command
            .get::<_, Option<String>>("last_error_code"),
        None
    );

    let archived_quarantined_command = &archived_commands[2];
    assert_eq!(
        archived_quarantined_command.get::<_, String>("consumer_name"),
        "search-indexer"
    );
    assert_eq!(
        archived_quarantined_command.get::<_, Uuid>("command_id"),
        quarantined_command_id
    );
    assert_eq!(
        archived_quarantined_command.get::<_, Uuid>("source_event_id"),
        event_id
    );
    assert_eq!(
        archived_quarantined_command.get::<_, chrono::DateTime<Utc>>("archived_at"),
        ts(200)
    );
    assert_eq!(
        archived_quarantined_command.get::<_, String>("command_type"),
        quarantined_command.command_type.as_str()
    );
    assert_eq!(
        archived_quarantined_command.get::<_, i32>("schema_version"),
        quarantined_command.schema_version
    );
    assert_eq!(
        archived_quarantined_command.get::<_, String>("status"),
        "quarantined"
    );
    assert_eq!(
        archived_quarantined_command.get::<_, i32>("attempt_count"),
        2
    );
    assert_eq!(
        archived_quarantined_command
            .get::<_, Option<String>>("payload_checksum")
            .as_deref(),
        Some(quarantined_command.payload_hash.as_str())
    );
    assert_eq!(
        archived_quarantined_command.get::<_, chrono::DateTime<Utc>>("received_at"),
        ts(44)
    );
    assert_eq!(
        archived_quarantined_command.get::<_, Option<chrono::DateTime<Utc>>>("completed_at"),
        None
    );
    assert_eq!(
        archived_quarantined_command
            .get::<_, Option<String>>("last_error_class")
            .as_deref(),
        Some("permanent")
    );
    assert_eq!(
        archived_quarantined_command
            .get::<_, Option<String>>("last_error_code")
            .as_deref(),
        Some("poison_command")
    );
    assert_eq!(
        archived_quarantined_command
            .get::<_, Option<String>>("last_error_detail")
            .as_deref(),
        Some("command payload rejected by projection worker")
    );
    assert_eq!(
        archived_quarantined_command
            .get::<_, Option<String>>("quarantine_reason")
            .as_deref(),
        Some("poison_pill")
    );
    assert_eq!(
        archived_quarantined_command
            .get::<_, Option<String>>("result_type"),
        None
    );
    assert_eq!(
        archived_quarantined_command.get::<_, Option<serde_json::Value>>("result_json"),
        None
    );
    assert_eq!(
        archived_quarantined_command.get::<_, Option<chrono::DateTime<Utc>>>("retain_until"),
        Some(ts(100))
    );

    let hot_eligible_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE (consumer_name = $1 AND command_id = $2)
               OR (consumer_name = $3 AND command_id = $4)
               OR (consumer_name = $5 AND command_id = $6)
            ",
            &[
                &"audit-projector",
                &completed_audit_command_id,
                &"projection-builder",
                &completed_projection_command_id,
                &"search-indexer",
                &quarantined_command_id,
            ],
        )
        .await
        .expect("failed to count hot eligible command inbox rows after prune")
        .get("count");
    let archived_eligible_command_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE (consumer_name = $1 AND command_id = $2)
               OR (consumer_name = $3 AND command_id = $4)
               OR (consumer_name = $5 AND command_id = $6)
            ",
            &[
                &"audit-projector",
                &completed_audit_command_id,
                &"projection-builder",
                &completed_projection_command_id,
                &"search-indexer",
                &quarantined_command_id,
            ],
        )
        .await
        .expect("failed to count archived eligible command inbox rows after prune")
        .get("count");
    let retained_terminal_hot_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
              AND status = 'completed'
              AND retain_until = $3
            ",
            &[
                &"projection-builder",
                &retained_terminal_command_id,
                &ts(300),
            ],
        )
        .await
        .expect("failed to count retained terminal command inbox row after prune")
        .get("count");
    let retained_terminal_archive_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"projection-builder", &retained_terminal_command_id],
        )
        .await
        .expect("failed to count retained terminal command archive row after prune")
        .get("count");
    let nonterminal_hot_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox
            WHERE consumer_name = $1
              AND command_id = $2
              AND status = 'processing'
              AND retain_until = $3
            ",
            &[&"audit-projector", &nonterminal_command_id, &ts(100)],
        )
        .await
        .expect("failed to count retained nonterminal command inbox row after prune")
        .get("count");
    let nonterminal_archive_count: i64 = tx
        .query_one(
            "
            SELECT COUNT(*) AS count
            FROM outbox.command_inbox_archive
            WHERE consumer_name = $1
              AND command_id = $2
            ",
            &[&"audit-projector", &nonterminal_command_id],
        )
        .await
        .expect("failed to count retained nonterminal command archive row after prune")
        .get("count");
    let hot_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.events WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count hot outbox event after command inbox completeness prune")
        .get("count");
    let archived_outbox_count: i64 = tx
        .query_one(
            "SELECT COUNT(*) AS count FROM outbox.outbox_event_archive WHERE event_id = $1",
            &[&event_id],
        )
        .await
        .expect("failed to count archived outbox event after command inbox completeness prune")
        .get("count");

    assert_eq!(hot_eligible_command_count, 0);
    assert_eq!(archived_eligible_command_count, 3);
    assert_eq!(retained_terminal_hot_count, 1);
    assert_eq!(retained_terminal_archive_count, 0);
    assert_eq!(nonterminal_hot_count, 1);
    assert_eq!(nonterminal_archive_count, 0);
    assert_eq!(hot_outbox_count, 0);
    assert_eq!(archived_outbox_count, 1);

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
