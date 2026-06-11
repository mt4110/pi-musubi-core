use chrono::{TimeZone, Utc};
use serde_json::json;
use tokio_postgres::NoTls;
use uuid::Uuid;

use musubi_orchestration::{
    AuthoritativeSqlCommand, CommandBeginOutcome, CommandEnvelope, NewOutboxMessage,
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
