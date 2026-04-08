use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    ArchivedCommandInboxEntry, ArchivedOutboxMessage, AuthoritativeChange, ClaimedOutboxMessage,
    CommandBeginOutcome, CommandCompletion, CommandEnvelope, CommandInboxEntry, CommandInboxStatus,
    CommandKey, DeliveryReceipt, NewOutboxMessage, OrchestrationError, OutboxAttempt,
    OutboxDeliveryStatus, OutboxMessage, ProcessingFailure, PruneOutcome, QuarantineReason,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriterReadSource {
    PrimaryWriter,
    ReadReplica,
}

pub trait OrchestrationStore {
    fn commit_authoritative_write(
        &mut self,
        source: WriterReadSource,
        change: AuthoritativeChange,
        message: NewOutboxMessage,
    ) -> Result<(), OrchestrationError>;

    fn claim_ready_outbox(
        &mut self,
        source: WriterReadSource,
        relay_name: &str,
        now: DateTime<Utc>,
        claimed_until: DateTime<Utc>,
    ) -> Result<Option<ClaimedOutboxMessage>, OrchestrationError>;

    fn mark_outbox_published(
        &mut self,
        event_id: Uuid,
        retain_until: DateTime<Utc>,
        receipt: DeliveryReceipt,
        attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError>;

    fn schedule_outbox_retry(
        &mut self,
        event_id: Uuid,
        retry_at: DateTime<Utc>,
        failure: ProcessingFailure,
        attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError>;

    fn quarantine_outbox(
        &mut self,
        event_id: Uuid,
        quarantined_at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        reason: QuarantineReason,
        failure: ProcessingFailure,
        attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError>;

    fn begin_command(
        &mut self,
        source: WriterReadSource,
        consumer_name: &str,
        command: CommandEnvelope,
        now: DateTime<Utc>,
        claimed_until: DateTime<Utc>,
    ) -> Result<CommandBeginOutcome, OrchestrationError>;

    fn complete_command(
        &mut self,
        consumer_name: &str,
        command_id: Uuid,
        expected_claimed_until: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        completion: CommandCompletion,
    ) -> Result<(), OrchestrationError>;

    fn schedule_command_retry(
        &mut self,
        consumer_name: &str,
        command_id: Uuid,
        expected_claimed_until: DateTime<Utc>,
        retry_at: DateTime<Utc>,
        failure: ProcessingFailure,
    ) -> Result<(), OrchestrationError>;

    fn quarantine_command(
        &mut self,
        consumer_name: &str,
        command_id: Uuid,
        expected_claimed_until: DateTime<Utc>,
        quarantined_at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        reason: QuarantineReason,
        failure: ProcessingFailure,
    ) -> Result<(), OrchestrationError>;

    fn prune_coordination(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<PruneOutcome, OrchestrationError>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryOrchestrationStore {
    authoritative_changes: Vec<AuthoritativeChange>,
    outbox_messages: HashMap<Uuid, OutboxMessage>,
    outbox_attempts: Vec<OutboxAttempt>,
    command_inbox: HashMap<CommandKey, CommandInboxEntry>,
    archived_outbox_messages: Vec<ArchivedOutboxMessage>,
    archived_command_inbox: Vec<ArchivedCommandInboxEntry>,
    next_causal_order: i64,
}

impl InMemoryOrchestrationStore {
    pub fn authoritative_changes(&self) -> &[AuthoritativeChange] {
        &self.authoritative_changes
    }

    pub fn outbox_message(&self, event_id: Uuid) -> Option<&OutboxMessage> {
        self.outbox_messages.get(&event_id)
    }

    pub fn outbox_attempts(&self, event_id: Uuid) -> Vec<&OutboxAttempt> {
        self.outbox_attempts
            .iter()
            .filter(|attempt| attempt.event_id == event_id)
            .collect()
    }

    pub fn command_inbox_entry(
        &self,
        consumer_name: &str,
        command_id: Uuid,
    ) -> Option<&CommandInboxEntry> {
        self.command_inbox.get(&CommandKey {
            consumer_name: consumer_name.to_owned(),
            command_id,
        })
    }

    pub fn archived_outbox_messages(&self) -> &[ArchivedOutboxMessage] {
        &self.archived_outbox_messages
    }

    pub fn archived_command_inbox(&self) -> &[ArchivedCommandInboxEntry] {
        &self.archived_command_inbox
    }

    fn ensure_writer(source: WriterReadSource) -> Result<(), OrchestrationError> {
        match source {
            WriterReadSource::PrimaryWriter => Ok(()),
            WriterReadSource::ReadReplica => Err(OrchestrationError::ReplicaReadForbidden),
        }
    }

    fn command_key(consumer_name: &str, command_id: Uuid) -> CommandKey {
        CommandKey {
            consumer_name: consumer_name.to_owned(),
            command_id,
        }
    }

    fn ensure_matching_command(
        consumer_name: &str,
        command: &CommandEnvelope,
        entry: &CommandInboxEntry,
    ) -> Result<(), OrchestrationError> {
        if command.matches_inbox_entry(entry) {
            return Ok(());
        }

        Err(OrchestrationError::ConflictingCommandEnvelope {
            consumer_name: consumer_name.to_owned(),
            command_id: command.command_id,
        })
    }

    fn outbox_attempt_matches(message: &OutboxMessage, attempt: &OutboxAttempt) -> bool {
        matches!(message.delivery_status, OutboxDeliveryStatus::Processing)
            && message.claimed_by.as_deref() == Some(attempt.relay_name.as_str())
            && message.claimed_until == Some(attempt.claimed_until)
    }

    fn command_claim_matches(
        entry: &CommandInboxEntry,
        consumer_name: &str,
        expected_claimed_until: DateTime<Utc>,
    ) -> bool {
        matches!(entry.status, CommandInboxStatus::Processing)
            && entry.claimed_by.as_deref() == Some(consumer_name)
            && entry.claimed_until == Some(expected_claimed_until)
    }
}

impl OrchestrationStore for InMemoryOrchestrationStore {
    fn commit_authoritative_write(
        &mut self,
        source: WriterReadSource,
        change: AuthoritativeChange,
        message: NewOutboxMessage,
    ) -> Result<(), OrchestrationError> {
        Self::ensure_writer(source)?;

        if self.outbox_messages.contains_key(&message.event_id) {
            return Err(OrchestrationError::EventAlreadyExists {
                event_id: message.event_id,
            });
        }

        if self
            .outbox_messages
            .values()
            .any(|existing| existing.idempotency_key == message.idempotency_key)
        {
            return Err(OrchestrationError::IdempotencyKeyAlreadyExists {
                idempotency_key: message.idempotency_key,
            });
        }

        self.next_causal_order += 1;
        self.authoritative_changes.push(change);
        self.outbox_messages.insert(
            message.event_id,
            OutboxMessage {
                event_id: message.event_id,
                idempotency_key: message.idempotency_key,
                stream_key: message.stream_key,
                aggregate_type: message.aggregate_type,
                aggregate_id: message.aggregate_id,
                event_type: message.event_type,
                schema_version: message.schema_version,
                payload_json: message.payload_json,
                payload_hash: message.payload_hash,
                delivery_status: OutboxDeliveryStatus::Pending,
                attempt_count: 0,
                available_at: message.available_at,
                created_at: message.created_at,
                published_at: None,
                last_attempt_at: None,
                last_error_class: None,
                last_error_code: None,
                last_error_detail: None,
                claimed_by: None,
                claimed_until: None,
                quarantined_at: None,
                quarantine_reason: None,
                retain_until: None,
                causal_order: self.next_causal_order,
                published_external_idempotency_key: None,
            },
        );

        Ok(())
    }

    fn claim_ready_outbox(
        &mut self,
        source: WriterReadSource,
        relay_name: &str,
        now: DateTime<Utc>,
        claimed_until: DateTime<Utc>,
    ) -> Result<Option<ClaimedOutboxMessage>, OrchestrationError> {
        Self::ensure_writer(source)?;

        let candidate_event_id = self
            .outbox_messages
            .values()
            .filter(|message| {
                matches!(
                    message.delivery_status,
                    OutboxDeliveryStatus::Pending | OutboxDeliveryStatus::Processing
                ) && message.available_at <= now
                    && match message.claimed_until {
                        Some(existing_claim) => existing_claim <= now,
                        None => true,
                    }
            })
            .min_by_key(|message| (message.available_at, message.causal_order))
            .map(|message| message.event_id);

        let Some(event_id) = candidate_event_id else {
            return Ok(None);
        };

        let message = self
            .outbox_messages
            .get_mut(&event_id)
            .ok_or(OrchestrationError::OutboxMessageNotFound { event_id })?;

        message.delivery_status = OutboxDeliveryStatus::Processing;
        message.claimed_by = Some(relay_name.to_owned());
        message.claimed_until = Some(claimed_until);

        Ok(Some(ClaimedOutboxMessage {
            message: message.clone(),
            relay_name: relay_name.to_owned(),
            claimed_at: now,
            claimed_until,
        }))
    }

    fn mark_outbox_published(
        &mut self,
        event_id: Uuid,
        retain_until: DateTime<Utc>,
        receipt: DeliveryReceipt,
        attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError> {
        let message = self
            .outbox_messages
            .get_mut(&event_id)
            .ok_or(OrchestrationError::OutboxMessageNotFound { event_id })?;

        if !Self::outbox_attempt_matches(message, &attempt) {
            return Err(OrchestrationError::StaleOutboxClaim { event_id });
        }

        message.delivery_status = OutboxDeliveryStatus::Published;
        message.attempt_count += 1;
        message.published_at = Some(attempt.finished_at);
        message.last_attempt_at = Some(attempt.finished_at);
        message.last_error_class = None;
        message.last_error_code = None;
        message.last_error_detail = None;
        message.claimed_by = None;
        message.claimed_until = None;
        message.retain_until = Some(retain_until);
        message.published_external_idempotency_key =
            Some(receipt.external_idempotency_key.as_str().to_owned());
        self.outbox_attempts.push(attempt);

        Ok(())
    }

    fn schedule_outbox_retry(
        &mut self,
        event_id: Uuid,
        retry_at: DateTime<Utc>,
        failure: ProcessingFailure,
        attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError> {
        let message = self
            .outbox_messages
            .get_mut(&event_id)
            .ok_or(OrchestrationError::OutboxMessageNotFound { event_id })?;

        if !Self::outbox_attempt_matches(message, &attempt) {
            return Err(OrchestrationError::StaleOutboxClaim { event_id });
        }

        message.delivery_status = OutboxDeliveryStatus::Pending;
        message.attempt_count += 1;
        message.available_at = retry_at;
        message.last_attempt_at = Some(attempt.finished_at);
        message.last_error_class = Some(failure.class);
        message.last_error_code = Some(failure.code.clone());
        message.last_error_detail = Some(failure.detail.clone());
        message.claimed_by = None;
        message.claimed_until = None;
        message.retain_until = None;
        self.outbox_attempts.push(attempt);

        Ok(())
    }

    fn quarantine_outbox(
        &mut self,
        event_id: Uuid,
        quarantined_at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        reason: QuarantineReason,
        failure: ProcessingFailure,
        attempt: OutboxAttempt,
    ) -> Result<(), OrchestrationError> {
        let message = self
            .outbox_messages
            .get_mut(&event_id)
            .ok_or(OrchestrationError::OutboxMessageNotFound { event_id })?;

        if !Self::outbox_attempt_matches(message, &attempt) {
            return Err(OrchestrationError::StaleOutboxClaim { event_id });
        }

        message.delivery_status = OutboxDeliveryStatus::Quarantined;
        message.attempt_count += 1;
        message.last_attempt_at = Some(attempt.finished_at);
        message.last_error_class = Some(failure.class);
        message.last_error_code = Some(failure.code.clone());
        message.last_error_detail = Some(failure.detail.clone());
        message.claimed_by = None;
        message.claimed_until = None;
        message.quarantined_at = Some(quarantined_at);
        message.quarantine_reason = Some(reason);
        message.retain_until = Some(retain_until);
        self.outbox_attempts.push(attempt);

        Ok(())
    }

    fn begin_command(
        &mut self,
        source: WriterReadSource,
        consumer_name: &str,
        command: CommandEnvelope,
        now: DateTime<Utc>,
        claimed_until: DateTime<Utc>,
    ) -> Result<CommandBeginOutcome, OrchestrationError> {
        Self::ensure_writer(source)?;

        let key = Self::command_key(consumer_name, command.command_id);

        if let Some(existing) = self.command_inbox.get_mut(&key) {
            Self::ensure_matching_command(consumer_name, &command, existing)?;

            let outcome = match existing.status {
                CommandInboxStatus::Completed | CommandInboxStatus::Quarantined => {
                    CommandBeginOutcome::Duplicate(existing.clone())
                }
                CommandInboxStatus::Pending if existing.available_at > now => {
                    CommandBeginOutcome::Deferred(existing.clone())
                }
                CommandInboxStatus::Pending | CommandInboxStatus::Processing => {
                    if matches!(existing.status, CommandInboxStatus::Processing)
                        && existing
                            .claimed_until
                            .is_some_and(|current_claim| current_claim > now)
                    {
                        let mut deferred = existing.clone();
                        deferred.available_at =
                            existing.claimed_until.unwrap_or(existing.available_at);
                        CommandBeginOutcome::Deferred(deferred)
                    } else {
                        existing.status = CommandInboxStatus::Processing;
                        existing.claimed_by = Some(consumer_name.to_owned());
                        existing.claimed_until = Some(claimed_until);
                        CommandBeginOutcome::ReadyForRetry(existing.clone())
                    }
                }
            };

            return Ok(outcome);
        }

        let entry = CommandInboxEntry {
            key: key.clone(),
            source_event_id: command.source_event_id,
            command_type: command.command_type,
            schema_version: command.schema_version,
            payload_hash: command.payload_hash,
            status: CommandInboxStatus::Processing,
            attempt_count: 0,
            first_seen_at: now,
            available_at: now,
            completed_at: None,
            claimed_by: Some(consumer_name.to_owned()),
            claimed_until: Some(claimed_until),
            last_error_class: None,
            last_error_code: None,
            last_error_detail: None,
            quarantined_at: None,
            quarantine_reason: None,
            result_type: None,
            result_json: None,
            retain_until: None,
        };

        self.command_inbox.insert(key, entry.clone());

        Ok(CommandBeginOutcome::FirstSeen(entry))
    }

    fn complete_command(
        &mut self,
        consumer_name: &str,
        command_id: Uuid,
        expected_claimed_until: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        completion: CommandCompletion,
    ) -> Result<(), OrchestrationError> {
        let key = Self::command_key(consumer_name, command_id);
        let entry = self.command_inbox.get_mut(&key).ok_or_else(|| {
            OrchestrationError::CommandInboxNotFound {
                consumer_name: consumer_name.to_owned(),
                command_id,
            }
        })?;

        if !Self::command_claim_matches(entry, consumer_name, expected_claimed_until) {
            return Err(OrchestrationError::StaleCommandClaim {
                consumer_name: consumer_name.to_owned(),
                command_id,
            });
        }

        entry.status = CommandInboxStatus::Completed;
        entry.attempt_count += 1;
        entry.completed_at = Some(completed_at);
        entry.claimed_by = None;
        entry.claimed_until = None;
        entry.last_error_class = None;
        entry.last_error_code = None;
        entry.last_error_detail = None;
        entry.result_type = Some(completion.result_type);
        entry.result_json = Some(completion.result_json);
        entry.retain_until = Some(retain_until);

        Ok(())
    }

    fn schedule_command_retry(
        &mut self,
        consumer_name: &str,
        command_id: Uuid,
        expected_claimed_until: DateTime<Utc>,
        retry_at: DateTime<Utc>,
        failure: ProcessingFailure,
    ) -> Result<(), OrchestrationError> {
        let key = Self::command_key(consumer_name, command_id);
        let entry = self.command_inbox.get_mut(&key).ok_or_else(|| {
            OrchestrationError::CommandInboxNotFound {
                consumer_name: consumer_name.to_owned(),
                command_id,
            }
        })?;

        if !Self::command_claim_matches(entry, consumer_name, expected_claimed_until) {
            return Err(OrchestrationError::StaleCommandClaim {
                consumer_name: consumer_name.to_owned(),
                command_id,
            });
        }

        entry.status = CommandInboxStatus::Pending;
        entry.attempt_count += 1;
        entry.available_at = retry_at;
        entry.claimed_by = None;
        entry.claimed_until = None;
        entry.last_error_class = Some(failure.class);
        entry.last_error_code = Some(failure.code);
        entry.last_error_detail = Some(failure.detail);
        entry.retain_until = None;

        Ok(())
    }

    fn quarantine_command(
        &mut self,
        consumer_name: &str,
        command_id: Uuid,
        expected_claimed_until: DateTime<Utc>,
        quarantined_at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        reason: QuarantineReason,
        failure: ProcessingFailure,
    ) -> Result<(), OrchestrationError> {
        let key = Self::command_key(consumer_name, command_id);
        let entry = self.command_inbox.get_mut(&key).ok_or_else(|| {
            OrchestrationError::CommandInboxNotFound {
                consumer_name: consumer_name.to_owned(),
                command_id,
            }
        })?;

        if !Self::command_claim_matches(entry, consumer_name, expected_claimed_until) {
            return Err(OrchestrationError::StaleCommandClaim {
                consumer_name: consumer_name.to_owned(),
                command_id,
            });
        }

        entry.status = CommandInboxStatus::Quarantined;
        entry.attempt_count += 1;
        entry.claimed_by = None;
        entry.claimed_until = None;
        entry.last_error_class = Some(failure.class);
        entry.last_error_code = Some(failure.code);
        entry.last_error_detail = Some(failure.detail);
        entry.quarantined_at = Some(quarantined_at);
        entry.quarantine_reason = Some(reason);
        entry.retain_until = Some(retain_until);

        Ok(())
    }

    fn prune_coordination(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<PruneOutcome, OrchestrationError> {
        let outbox_event_ids: Vec<Uuid> = self
            .outbox_messages
            .values()
            .filter(|message| {
                matches!(
                    message.delivery_status,
                    OutboxDeliveryStatus::Published | OutboxDeliveryStatus::Quarantined
                ) && message
                    .retain_until
                    .is_some_and(|retain_until| retain_until <= now)
            })
            .map(|message| message.event_id)
            .collect();

        for event_id in &outbox_event_ids {
            if let Some(message) = self.outbox_messages.remove(event_id) {
                let attempts: Vec<OutboxAttempt> = self
                    .outbox_attempts
                    .iter()
                    .filter(|attempt| attempt.event_id == *event_id)
                    .cloned()
                    .collect();
                self.outbox_attempts
                    .retain(|attempt| attempt.event_id != *event_id);
                self.archived_outbox_messages.push(ArchivedOutboxMessage {
                    message,
                    attempts,
                    pruned_at: now,
                });
            }
        }

        let command_keys: Vec<CommandKey> = self
            .command_inbox
            .values()
            .filter(|entry| {
                matches!(
                    entry.status,
                    CommandInboxStatus::Completed | CommandInboxStatus::Quarantined
                ) && entry
                    .retain_until
                    .is_some_and(|retain_until| retain_until <= now)
            })
            .map(|entry| entry.key.clone())
            .collect();

        for command_key in &command_keys {
            if let Some(entry) = self.command_inbox.remove(command_key) {
                self.archived_command_inbox.push(ArchivedCommandInboxEntry {
                    entry,
                    pruned_at: now,
                });
            }
        }

        Ok(PruneOutcome {
            pruned_outbox_event_ids: outbox_event_ids,
            pruned_command_keys: command_keys,
        })
    }
}
