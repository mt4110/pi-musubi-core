use std::{future::Future, pin::Pin};

use chrono::{DateTime, Utc};
use tokio_postgres::{Row, Transaction};
use uuid::Uuid;

use crate::{
    CommandBeginOutcome, CommandEnvelope, CommandInboxEntry, CommandInboxStatus, CommandKey,
    NewOutboxMessage, OrchestrationError, OutboxDeliveryStatus, OutboxMessage, PruneOutcome,
    QuarantineReason, RetryClass,
};

pub struct PostgresOrchestrationStore;

impl PostgresOrchestrationStore {
    pub async fn record_authoritative_write<Apply>(
        tx: &Transaction<'_>,
        message: &NewOutboxMessage,
        apply_authoritative_write: Apply,
    ) -> Result<(), OrchestrationError>
    where
        Apply: for<'a> FnOnce(
            &'a Transaction<'a>,
        )
            -> Pin<Box<dyn Future<Output = Result<(), OrchestrationError>> + 'a>>,
    {
        apply_authoritative_write(tx).await?;
        Self::insert_outbox_message(tx, message).await
    }

    pub async fn insert_outbox_message(
        tx: &Transaction<'_>,
        message: &NewOutboxMessage,
    ) -> Result<(), OrchestrationError> {
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
                available_at,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', 0, $10, $11)
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
                &message.available_at,
                &message.created_at,
            ],
        )
        .await
        .map_err(database_error)?;

        Ok(())
    }

    pub async fn begin_command(
        tx: &Transaction<'_>,
        consumer_name: &str,
        command: &CommandEnvelope,
        now: DateTime<Utc>,
        claimed_until: DateTime<Utc>,
    ) -> Result<CommandBeginOutcome, OrchestrationError> {
        let inserted = tx
            .query_opt(
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
                    claimed_by,
                    claimed_until,
                    command_type,
                    schema_version
                )
                VALUES ($1, $2, $3, $4, $5, $6, 'processing', $6, 0, $2, $7, $8, $9)
                ON CONFLICT (consumer_name, command_id) DO NOTHING
                RETURNING *
                ",
                &[
                    &Uuid::new_v4(),
                    &consumer_name,
                    &command.command_id,
                    &command.source_event_id,
                    &command.payload_hash,
                    &now,
                    &claimed_until,
                    &command.command_type,
                    &command.schema_version,
                ],
            )
            .await
            .map_err(database_error)?;

        if let Some(row) = inserted {
            return Ok(CommandBeginOutcome::FirstSeen(map_command_row(row)?));
        }

        let existing = map_command_row(
            tx.query_one(
                "
                SELECT *
                FROM outbox.command_inbox
                WHERE consumer_name = $1
                  AND command_id = $2
                FOR UPDATE
                ",
                &[&consumer_name, &command.command_id],
            )
            .await
            .map_err(database_error)?,
        )?;

        if !command.matches_inbox_entry(&existing) {
            return Err(OrchestrationError::ConflictingCommandEnvelope {
                consumer_name: consumer_name.to_owned(),
                command_id: command.command_id,
            });
        }

        match existing.status {
            CommandInboxStatus::Completed | CommandInboxStatus::Quarantined => {
                Ok(CommandBeginOutcome::Duplicate(existing))
            }
            CommandInboxStatus::Pending if existing.available_at > now => {
                Ok(CommandBeginOutcome::Deferred(existing))
            }
            CommandInboxStatus::Pending | CommandInboxStatus::Processing => {
                if matches!(existing.status, CommandInboxStatus::Processing)
                    && existing.claimed_until.is_some_and(|until| until > now)
                {
                    let mut deferred = existing.clone();
                    deferred.available_at = existing.claimed_until.unwrap_or(existing.available_at);
                    return Ok(CommandBeginOutcome::Deferred(deferred));
                }

                let row = tx
                    .query_one(
                        "
                        UPDATE outbox.command_inbox
                        SET status = 'processing',
                            claimed_by = $3,
                            claimed_until = $4
                        WHERE consumer_name = $1
                          AND command_id = $2
                        RETURNING *
                        ",
                        &[
                            &consumer_name,
                            &command.command_id,
                            &consumer_name,
                            &claimed_until,
                        ],
                    )
                    .await
                    .map_err(database_error)?;

                Ok(CommandBeginOutcome::ReadyForRetry(map_command_row(row)?))
            }
        }
    }

    pub async fn prune_coordination(
        tx: &Transaction<'_>,
        now: DateTime<Utc>,
    ) -> Result<PruneOutcome, OrchestrationError> {
        let outbox_ids = tx
            .query(
                "
                SELECT event_id
                FROM outbox.events
                WHERE delivery_status IN ('published', 'quarantined')
                  AND retain_until IS NOT NULL
                  AND retain_until <= $1
                ORDER BY causal_order
                ",
                &[&now],
            )
            .await
            .map_err(database_error)?
            .into_iter()
            .map(|row| row.get::<_, Uuid>("event_id"))
            .collect::<Vec<_>>();

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
            SELECT
                event_id,
                $1,
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
                quarantined_at,
                last_attempt_at,
                last_error_class,
                last_error_code,
                last_error_detail,
                quarantine_reason,
                retain_until,
                published_external_idempotency_key
            FROM outbox.events
            WHERE delivery_status IN ('published', 'quarantined')
              AND retain_until IS NOT NULL
              AND retain_until <= $1
            ON CONFLICT (event_id) DO NOTHING
            ",
            &[&now],
        )
        .await
        .map_err(database_error)?;

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
            SELECT
                attempts.event_id,
                attempts.attempt_number,
                $1,
                attempts.relay_name,
                attempts.claimed_at,
                attempts.claimed_until,
                attempts.finished_at,
                attempts.failure_class,
                attempts.failure_code,
                attempts.failure_detail,
                attempts.external_idempotency_key
            FROM outbox.outbox_attempts AS attempts
            JOIN outbox.events AS events
              ON events.event_id = attempts.event_id
            WHERE events.delivery_status IN ('published', 'quarantined')
              AND events.retain_until IS NOT NULL
              AND events.retain_until <= $1
            ON CONFLICT (event_id, attempt_number) DO NOTHING
            ",
            &[&now],
        )
        .await
        .map_err(database_error)?;

        tx.execute(
            "
            DELETE FROM outbox.events
            WHERE delivery_status IN ('published', 'quarantined')
              AND retain_until IS NOT NULL
              AND retain_until <= $1
            ",
            &[&now],
        )
        .await
        .map_err(database_error)?;

        let command_rows = tx
            .query(
                "
                SELECT consumer_name, command_id
                FROM outbox.command_inbox
                WHERE status IN ('completed', 'quarantined')
                  AND retain_until IS NOT NULL
                  AND retain_until <= $1
                ORDER BY consumer_name, command_id
                ",
                &[&now],
            )
            .await
            .map_err(database_error)?;

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
            SELECT
                consumer_name,
                command_id,
                source_event_id,
                $1,
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
            FROM outbox.command_inbox
            WHERE status IN ('completed', 'quarantined')
              AND retain_until IS NOT NULL
              AND retain_until <= $1
            ON CONFLICT (consumer_name, command_id) DO NOTHING
            ",
            &[&now],
        )
        .await
        .map_err(database_error)?;

        tx.execute(
            "
            DELETE FROM outbox.command_inbox
            WHERE status IN ('completed', 'quarantined')
              AND retain_until IS NOT NULL
              AND retain_until <= $1
            ",
            &[&now],
        )
        .await
        .map_err(database_error)?;

        Ok(PruneOutcome {
            pruned_outbox_event_ids: outbox_ids,
            pruned_command_keys: command_rows
                .into_iter()
                .map(|row| CommandKey {
                    consumer_name: row.get("consumer_name"),
                    command_id: row.get("command_id"),
                })
                .collect(),
        })
    }
}

fn map_command_row(row: Row) -> Result<CommandInboxEntry, OrchestrationError> {
    let payload_hash = row
        .get::<_, Option<String>>("payload_checksum")
        .ok_or_else(|| {
            OrchestrationError::Database("command inbox row is missing payload_checksum".to_owned())
        })?;

    Ok(CommandInboxEntry {
        key: CommandKey {
            consumer_name: row.get("consumer_name"),
            command_id: row.get("command_id"),
        },
        source_event_id: row.get("source_event_id"),
        command_type: row.get("command_type"),
        schema_version: row.get("schema_version"),
        payload_hash,
        status: parse_command_status(row.get::<_, String>("status").as_str())?,
        attempt_count: row.get::<_, i32>("attempt_count") as u32,
        first_seen_at: row.get("received_at"),
        available_at: row.get("available_at"),
        completed_at: row.get("completed_at"),
        claimed_by: row.get("claimed_by"),
        claimed_until: row.get("claimed_until"),
        last_error_class: row
            .get::<_, Option<String>>("last_error_class")
            .map(|value| parse_retry_class(value.as_str()))
            .transpose()?,
        last_error_code: row.get("last_error_code"),
        last_error_detail: row.get("last_error_detail"),
        quarantined_at: row.get("quarantined_at"),
        quarantine_reason: row
            .get::<_, Option<String>>("quarantine_reason")
            .map(|value| parse_quarantine_reason(value.as_str()))
            .transpose()?,
        result_type: row.get("result_type"),
        result_json: row.get("result_json"),
        retain_until: row.get("retain_until"),
    })
}

fn parse_command_status(value: &str) -> Result<CommandInboxStatus, OrchestrationError> {
    match value {
        "pending" => Ok(CommandInboxStatus::Pending),
        "processing" => Ok(CommandInboxStatus::Processing),
        "completed" => Ok(CommandInboxStatus::Completed),
        "quarantined" => Ok(CommandInboxStatus::Quarantined),
        other => Err(OrchestrationError::Database(format!(
            "unknown command inbox status: {other}"
        ))),
    }
}

fn parse_retry_class(value: &str) -> Result<RetryClass, OrchestrationError> {
    match value {
        "transient" => Ok(RetryClass::Transient),
        "permanent" => Ok(RetryClass::Permanent),
        "deferred" => Ok(RetryClass::Deferred),
        other => Err(OrchestrationError::Database(format!(
            "unknown retry class: {other}"
        ))),
    }
}

fn parse_quarantine_reason(value: &str) -> Result<QuarantineReason, OrchestrationError> {
    match value {
        "poison_pill" => Ok(QuarantineReason::PoisonPill),
        "permanent_failure" => Ok(QuarantineReason::PermanentFailure),
        "attempt_budget_exceeded" => Ok(QuarantineReason::AttemptBudgetExceeded),
        "compatibility_window_expired" => Ok(QuarantineReason::CompatibilityWindowExpired),
        other => Err(OrchestrationError::Database(format!(
            "unknown quarantine reason: {other}"
        ))),
    }
}

#[allow(dead_code)]
fn parse_outbox_status(value: &str) -> Result<OutboxDeliveryStatus, OrchestrationError> {
    match value {
        "pending" => Ok(OutboxDeliveryStatus::Pending),
        "processing" => Ok(OutboxDeliveryStatus::Processing),
        "published" => Ok(OutboxDeliveryStatus::Published),
        "failed" => Ok(OutboxDeliveryStatus::Failed),
        "quarantined" => Ok(OutboxDeliveryStatus::Quarantined),
        other => Err(OrchestrationError::Database(format!(
            "unknown outbox delivery status: {other}"
        ))),
    }
}

#[allow(dead_code)]
fn parse_outbox_row(row: Row) -> Result<OutboxMessage, OrchestrationError> {
    Ok(OutboxMessage {
        event_id: row.get("event_id"),
        idempotency_key: row.get("idempotency_key"),
        stream_key: row.get("stream_key"),
        aggregate_type: row.get("aggregate_type"),
        aggregate_id: row.get("aggregate_id"),
        event_type: row.get("event_type"),
        schema_version: row.get("schema_version"),
        payload_json: row.get("payload_json"),
        payload_hash: row.get("payload_hash"),
        delivery_status: parse_outbox_status(row.get::<_, String>("delivery_status").as_str())?,
        attempt_count: row.get::<_, i32>("attempt_count") as u32,
        available_at: row.get("available_at"),
        created_at: row.get("created_at"),
        published_at: row.get("published_at"),
        last_attempt_at: row.get("last_attempt_at"),
        last_error_class: row
            .get::<_, Option<String>>("last_error_class")
            .map(|value| parse_retry_class(value.as_str()))
            .transpose()?,
        last_error_code: row.get("last_error_code"),
        last_error_detail: row.get("last_error_detail"),
        claimed_by: row.get("claimed_by"),
        claimed_until: row.get("claimed_until"),
        quarantined_at: row.get("quarantined_at"),
        quarantine_reason: row
            .get::<_, Option<String>>("quarantine_reason")
            .map(|value| parse_quarantine_reason(value.as_str()))
            .transpose()?,
        retain_until: row.get("retain_until"),
        causal_order: row.get("causal_order"),
        published_external_idempotency_key: row.get("published_external_idempotency_key"),
    })
}

fn database_error(error: tokio_postgres::Error) -> OrchestrationError {
    OrchestrationError::Database(error.to_string())
}
