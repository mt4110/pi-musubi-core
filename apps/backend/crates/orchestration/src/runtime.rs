use std::future::Future;

use chrono::{DateTime, TimeDelta, Utc};

use crate::{
    AuthoritativeChange, ClaimedOutboxMessage, CommandBeginOutcome, CommandCompletion,
    CommandEnvelope, ConsumeOutcome, DeliveryOutcome, DeliveryReceipt, NewOutboxMessage,
    OrchestrationError, OrchestrationStore, OutboxAttempt, ProcessingFailure, QuarantineReason,
    RetentionPolicy, RetryClass, RetryPolicy, SchemaCompatibilityPolicy, WriterReadSource,
};

pub struct OrchestrationRuntime<Store> {
    store: Store,
    retry_policy: RetryPolicy,
    retention_policy: RetentionPolicy,
    schema_policy: SchemaCompatibilityPolicy,
    claim_lease_for: TimeDelta,
}

impl<Store> OrchestrationRuntime<Store> {
    pub fn new(
        store: Store,
        retry_policy: RetryPolicy,
        retention_policy: RetentionPolicy,
        schema_policy: SchemaCompatibilityPolicy,
        claim_lease_for: TimeDelta,
    ) -> Self {
        Self {
            store,
            retry_policy,
            retention_policy,
            schema_policy,
            claim_lease_for,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    pub fn into_store(self) -> Store {
        self.store
    }
}

impl<Store> OrchestrationRuntime<Store>
where
    Store: OrchestrationStore,
{
    pub fn record_authoritative_write(
        &mut self,
        change: AuthoritativeChange,
        message: NewOutboxMessage,
    ) -> Result<(), OrchestrationError> {
        self.store
            .commit_authoritative_write(WriterReadSource::PrimaryWriter, change, message)
    }

    pub async fn deliver_ready_outbox<Publish, Fut>(
        &mut self,
        relay_name: &str,
        now: DateTime<Utc>,
        publish: Publish,
    ) -> Result<DeliveryOutcome, OrchestrationError>
    where
        Publish: FnOnce(ClaimedOutboxMessage) -> Fut,
        Fut: Future<Output = Result<DeliveryReceipt, ProcessingFailure>>,
    {
        let Some(claimed) = self.store.claim_ready_outbox(
            WriterReadSource::PrimaryWriter,
            relay_name,
            now,
            now + self.claim_lease_for,
        )?
        else {
            return Ok(DeliveryOutcome::Idle);
        };

        let next_attempt_number = claimed.message.attempt_count + 1;
        if let Err(failure) = self.schema_policy.classify(
            claimed.message.schema_version,
            claimed.message.created_at,
            now,
        ) {
            let attempt = OutboxAttempt {
                event_id: claimed.message.event_id,
                attempt_number: next_attempt_number,
                relay_name: relay_name.to_owned(),
                claimed_at: claimed.claimed_at,
                claimed_until: claimed.claimed_until,
                finished_at: now,
                failure_class: Some(failure.class),
                failure_code: Some(failure.code.clone()),
                failure_detail: Some(failure.detail.clone()),
                external_idempotency_key: None,
            };

            return self.finish_outbox_failure(
                claimed.message.event_id,
                now,
                next_attempt_number,
                failure,
                attempt,
            );
        }

        let publish_result = publish(claimed.clone()).await;

        match publish_result {
            Ok(receipt) => {
                match self.store.mark_outbox_published(
                    claimed.message.event_id,
                    now + self.retention_policy.published_outbox_for,
                    receipt.clone(),
                    OutboxAttempt {
                        event_id: claimed.message.event_id,
                        attempt_number: next_attempt_number,
                        relay_name: relay_name.to_owned(),
                        claimed_at: claimed.claimed_at,
                        claimed_until: claimed.claimed_until,
                        finished_at: now,
                        failure_class: None,
                        failure_code: None,
                        failure_detail: None,
                        external_idempotency_key: Some(
                            receipt.external_idempotency_key.as_str().to_owned(),
                        ),
                    },
                ) {
                    Ok(()) => Ok(DeliveryOutcome::Published {
                        event_id: claimed.message.event_id,
                    }),
                    Err(OrchestrationError::StaleOutboxClaim { .. }) => Ok(DeliveryOutcome::Idle),
                    Err(error) => Err(error),
                }
            }
            Err(failure) => {
                let attempt = OutboxAttempt {
                    event_id: claimed.message.event_id,
                    attempt_number: next_attempt_number,
                    relay_name: relay_name.to_owned(),
                    claimed_at: claimed.claimed_at,
                    claimed_until: claimed.claimed_until,
                    finished_at: now,
                    failure_class: Some(failure.class),
                    failure_code: Some(failure.code.clone()),
                    failure_detail: Some(failure.detail.clone()),
                    external_idempotency_key: None,
                };

                self.finish_outbox_failure(
                    claimed.message.event_id,
                    now,
                    next_attempt_number,
                    failure,
                    attempt,
                )
            }
        }
    }

    pub async fn consume_command<Handle, Fut>(
        &mut self,
        consumer_name: &str,
        command: CommandEnvelope,
        now: DateTime<Utc>,
        handle: Handle,
    ) -> Result<ConsumeOutcome, OrchestrationError>
    where
        Handle: FnOnce(CommandEnvelope) -> Fut,
        Fut: Future<Output = Result<CommandCompletion, ProcessingFailure>>,
    {
        let command_id = command.command_id;
        let begin_outcome = self.store.begin_command(
            WriterReadSource::PrimaryWriter,
            consumer_name,
            command.clone(),
            now,
            now + self.claim_lease_for,
        )?;

        match begin_outcome {
            CommandBeginOutcome::Duplicate(_) => Ok(ConsumeOutcome::Duplicate { command_id }),
            CommandBeginOutcome::Deferred(entry) => Ok(ConsumeOutcome::Deferred {
                command_id,
                retry_at: entry.available_at,
            }),
            CommandBeginOutcome::FirstSeen(entry) | CommandBeginOutcome::ReadyForRetry(entry) => {
                let claimed_until = entry.claimed_until.ok_or_else(|| {
                    OrchestrationError::Database(
                        "command begin outcome is missing the active claim lease".to_owned(),
                    )
                })?;
                let next_attempt_number = entry.attempt_count + 1;
                if let Err(failure) =
                    self.schema_policy
                        .classify(entry.schema_version, entry.first_seen_at, now)
                {
                    return self.finish_command_failure(
                        consumer_name,
                        command_id,
                        claimed_until,
                        now,
                        next_attempt_number,
                        failure,
                    );
                }

                let handle_result = handle(command).await;

                match handle_result {
                    Ok(completion) => match self.store.complete_command(
                        consumer_name,
                        command_id,
                        claimed_until,
                        now,
                        now + self.retention_policy.completed_command_for,
                        completion,
                    ) {
                        Ok(()) => Ok(ConsumeOutcome::Completed { command_id }),
                        Err(OrchestrationError::StaleCommandClaim { .. }) => {
                            Ok(ConsumeOutcome::Deferred {
                                command_id,
                                retry_at: claimed_until,
                            })
                        }
                        Err(error) => Err(error),
                    },
                    Err(failure) => self.finish_command_failure(
                        consumer_name,
                        command_id,
                        claimed_until,
                        now,
                        next_attempt_number,
                        failure,
                    ),
                }
            }
        }
    }

    pub fn prune_coordination(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<crate::PruneOutcome, OrchestrationError> {
        self.store.prune_coordination(now)
    }

    fn finish_outbox_failure(
        &mut self,
        event_id: uuid::Uuid,
        now: DateTime<Utc>,
        next_attempt_number: u32,
        failure: ProcessingFailure,
        attempt: OutboxAttempt,
    ) -> Result<DeliveryOutcome, OrchestrationError> {
        let should_retry = match failure.class {
            RetryClass::Transient => next_attempt_number < self.retry_policy.max_attempts,
            RetryClass::Deferred => true,
            RetryClass::Permanent => false,
        };

        if should_retry {
            let retry_at = self
                .retry_policy
                .next_retry_at(now, event_id, next_attempt_number);

            self.store
                .schedule_outbox_retry(event_id, retry_at, failure, attempt)
                .map(|()| DeliveryOutcome::RetryScheduled { event_id, retry_at })
                .or_else(|error| match error {
                    OrchestrationError::StaleOutboxClaim { .. } => Ok(DeliveryOutcome::Idle),
                    other => Err(other),
                })
        } else {
            let reason = if matches!(failure.class, RetryClass::Transient) {
                QuarantineReason::AttemptBudgetExceeded
            } else {
                failure.quarantine_reason
            };

            self.store
                .quarantine_outbox(
                    event_id,
                    now,
                    now + self.retention_policy.quarantined_outbox_for,
                    reason,
                    failure,
                    attempt,
                )
                .map(|()| DeliveryOutcome::Quarantined { event_id, reason })
                .or_else(|error| match error {
                    OrchestrationError::StaleOutboxClaim { .. } => Ok(DeliveryOutcome::Idle),
                    other => Err(other),
                })
        }
    }

    fn finish_command_failure(
        &mut self,
        consumer_name: &str,
        command_id: uuid::Uuid,
        expected_claimed_until: DateTime<Utc>,
        now: DateTime<Utc>,
        next_attempt_number: u32,
        failure: ProcessingFailure,
    ) -> Result<ConsumeOutcome, OrchestrationError> {
        let should_retry = match failure.class {
            RetryClass::Transient => next_attempt_number < self.retry_policy.max_attempts,
            RetryClass::Deferred => true,
            RetryClass::Permanent => false,
        };

        if should_retry {
            let retry_at = self
                .retry_policy
                .next_retry_at(now, command_id, next_attempt_number);

            self.store
                .schedule_command_retry(
                    consumer_name,
                    command_id,
                    expected_claimed_until,
                    retry_at,
                    failure,
                )
                .map(|()| ConsumeOutcome::RetryScheduled {
                    command_id,
                    retry_at,
                })
                .or_else(|error| match error {
                    OrchestrationError::StaleCommandClaim { .. } => Ok(ConsumeOutcome::Deferred {
                        command_id,
                        retry_at: expected_claimed_until,
                    }),
                    other => Err(other),
                })
        } else {
            let reason = if matches!(failure.class, RetryClass::Transient) {
                QuarantineReason::AttemptBudgetExceeded
            } else {
                failure.quarantine_reason
            };

            self.store
                .quarantine_command(
                    consumer_name,
                    command_id,
                    expected_claimed_until,
                    now,
                    now + self.retention_policy.quarantined_command_for,
                    reason,
                    failure,
                )
                .map(|()| ConsumeOutcome::Quarantined { command_id, reason })
                .or_else(|error| match error {
                    OrchestrationError::StaleCommandClaim { .. } => Ok(ConsumeOutcome::Deferred {
                        command_id,
                        retry_at: expected_claimed_until,
                    }),
                    other => Err(other),
                })
        }
    }
}
