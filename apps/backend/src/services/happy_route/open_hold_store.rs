use chrono::Utc;
use musubi_settlement_domain::SubmissionResult;
use uuid::Uuid;

use super::{
    authoritative::append_normalized_observations,
    constants::{EVENT_REFRESH_SETTLEMENT_VIEW, SETTLEMENT_ORCHESTRATOR},
    outbox::{insert_outbox_message, mark_outbox_published},
    repository::HappyRouteWriteRepository,
    state::{
        CommandInboxRecord, OutboxCommand, OutboxMessageRecord, SettlementIntentRecord,
        SettlementSubmissionRecord,
    },
    types::{
        HappyRouteError, OpenHoldIntentPersistResult, OpenHoldIntentPrepareOutcome,
        SubmissionPreparation, processed_outbox_message,
    },
};

impl<'a> HappyRouteWriteRepository<'a> {
    pub(super) fn prepare_open_hold_intent(
        &mut self,
        message: &OutboxMessageRecord,
        settlement_case_id: &str,
    ) -> Result<OpenHoldIntentPrepareOutcome, HappyRouteError> {
        let inbox_key = (SETTLEMENT_ORCHESTRATOR.to_owned(), message.event_id.clone());
        if self.store.command_inbox_by_key.contains_key(&inbox_key) {
            mark_outbox_published(self.store, &message.event_id);
            return Ok(OpenHoldIntentPrepareOutcome::ReplayNoop(
                processed_outbox_message(message, SETTLEMENT_ORCHESTRATOR, None, true),
            ));
        }

        self.store.command_inbox_by_key.insert(
            inbox_key,
            CommandInboxRecord {
                consumer_name: SETTLEMENT_ORCHESTRATOR.to_owned(),
                source_message_id: message.event_id.clone(),
                received_at: Utc::now(),
                processed_at: None,
            },
        );

        let settlement_case = self
            .store
            .settlement_cases_by_id
            .get(settlement_case_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::NotFound(
                    "settlement case referenced by outbox is missing".to_owned(),
                )
            })?;
        let promise_intent = self
            .store
            .promise_intents_by_id
            .get(&settlement_case.promise_intent_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::NotFound(
                    "promise intent referenced by settlement case is missing".to_owned(),
                )
            })?;

        let settlement_intent_id = Uuid::new_v4().to_string();
        let settlement_submission_id = Uuid::new_v4().to_string();
        let internal_idempotency_key = format!("hold-intent:{}", message.idempotency_key);

        self.store.settlement_intents_by_id.insert(
            settlement_intent_id.clone(),
            SettlementIntentRecord {
                settlement_intent_id: settlement_intent_id.clone(),
                settlement_case_id: settlement_case_id.to_owned(),
                capability: "HoldValue".to_owned(),
                internal_idempotency_key: internal_idempotency_key.clone(),
                created_at: Utc::now(),
            },
        );
        self.store.settlement_submissions_by_id.insert(
            settlement_submission_id.clone(),
            SettlementSubmissionRecord {
                settlement_submission_id: settlement_submission_id.clone(),
                settlement_case_id: settlement_case_id.to_owned(),
                settlement_intent_id: settlement_intent_id.clone(),
                provider_submission_id: None,
                provider_ref: None,
                provider_idempotency_key: String::new(),
                submission_status: "pending".to_owned(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        );

        Ok(OpenHoldIntentPrepareOutcome::Ready(SubmissionPreparation {
            settlement_case,
            promise_intent,
            settlement_intent_id,
            settlement_submission_id,
            internal_idempotency_key,
        }))
    }

    pub(super) fn persist_open_hold_intent_result(
        &mut self,
        message: &OutboxMessageRecord,
        prepare: &SubmissionPreparation,
        submission_result: SubmissionResult,
    ) -> Result<OpenHoldIntentPersistResult, HappyRouteError> {
        let submission = self
            .store
            .settlement_submissions_by_id
            .get_mut(&prepare.settlement_submission_id)
            .ok_or_else(|| {
                HappyRouteError::Internal(
                    "pending settlement submission disappeared before persistence".to_owned(),
                )
            })?;

        let mut provider_submission_id = None;

        match submission_result {
            SubmissionResult::Accepted {
                provider_ref,
                provider_submission_id: accepted_submission_id,
                provider_idempotency_key,
                observations,
                ..
            } => {
                let accepted_provider_submission_id = accepted_submission_id
                    .as_ref()
                    .map(|value| value.as_str().to_owned());
                submission.provider_submission_id = accepted_provider_submission_id.clone();
                submission.provider_ref =
                    provider_ref.as_ref().map(|value| value.as_str().to_owned());
                submission.provider_idempotency_key = provider_idempotency_key.as_str().to_owned();
                submission.submission_status = "accepted".to_owned();
                submission.updated_at = Utc::now();

                if let Some(provider_submission_id_value) = accepted_provider_submission_id.clone()
                {
                    self.store
                        .settlement_submission_id_by_provider_submission_id
                        .insert(
                            provider_submission_id_value.clone(),
                            prepare.settlement_submission_id.clone(),
                        );
                    provider_submission_id = Some(provider_submission_id_value);
                }

                append_normalized_observations(
                    self.store,
                    &prepare.settlement_case.settlement_case_id,
                    Some(&prepare.settlement_submission_id),
                    &observations,
                );
            }
            SubmissionResult::Deferred {
                provider_idempotency_key,
                observations,
                ..
            } => {
                submission.provider_idempotency_key = provider_idempotency_key.as_str().to_owned();
                submission.submission_status = "deferred".to_owned();
                submission.updated_at = Utc::now();
                append_normalized_observations(
                    self.store,
                    &prepare.settlement_case.settlement_case_id,
                    Some(&prepare.settlement_submission_id),
                    &observations,
                );
            }
            SubmissionResult::RejectedPermanent { observations, .. } => {
                submission.submission_status = "rejected".to_owned();
                submission.updated_at = Utc::now();
                append_normalized_observations(
                    self.store,
                    &prepare.settlement_case.settlement_case_id,
                    Some(&prepare.settlement_submission_id),
                    &observations,
                );
            }
            SubmissionResult::NeedsManualReview { observations, .. } => {
                submission.submission_status = "manual_review".to_owned();
                submission.updated_at = Utc::now();
                append_normalized_observations(
                    self.store,
                    &prepare.settlement_case.settlement_case_id,
                    Some(&prepare.settlement_submission_id),
                    &observations,
                );
            }
            _ => {
                return Err(HappyRouteError::Internal(
                    "submission result returned an unsupported non-exhaustive variant".to_owned(),
                ));
            }
        }

        if let Some(command_inbox) = self
            .store
            .command_inbox_by_key
            .get_mut(&(SETTLEMENT_ORCHESTRATOR.to_owned(), message.event_id.clone()))
        {
            command_inbox.processed_at = Some(Utc::now());
        }

        insert_outbox_message(
            self.store,
            "settlement_case",
            &prepare.settlement_case.settlement_case_id,
            EVENT_REFRESH_SETTLEMENT_VIEW,
            OutboxCommand::RefreshSettlementView {
                settlement_case_id: prepare.settlement_case.settlement_case_id.clone(),
            },
        );
        mark_outbox_published(self.store, &message.event_id);

        Ok(OpenHoldIntentPersistResult {
            provider_submission_id,
        })
    }
}
