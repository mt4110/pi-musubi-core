use chrono::{TimeZone, Utc};
use serde_json::json;
use tokio_postgres::NoTls;
use uuid::Uuid;

use musubi_orchestration::{
    CommandBeginOutcome, CommandEnvelope, NewOutboxMessage, OrchestrationError,
    PostgresOrchestrationStore,
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
    let message = NewOutboxMessage::new(
        event_id,
        Uuid::from_u128(0x703),
        "settlement_case:700",
        "settlement_case",
        fact_id,
        "settlement.receipt_recorded",
        1,
        json!({ "fact_id": fact_id }),
        ts(0),
        ts(0),
    )
    .unwrap();

    PostgresOrchestrationStore::record_authoritative_write(&tx, &message, |tx| {
        Box::pin(async move {
            tx.execute(
                "INSERT INTO authoritative_facts (fact_id, fact_kind) VALUES ($1, $2)",
                &[&fact_id, &"receipt_recorded"],
            )
            .await
            .map_err(|error| OrchestrationError::Database(error.to_string()))?;
            Ok(())
        })
    })
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
