use chrono::Utc;
use musubi_settlement_domain::{Money, NormalizedObservation, ReceiptVerification};
use uuid::Uuid;

use super::{
    authoritative::{append_normalized_observations, append_receipt_recognition_ledger},
    constants::{
        EVENT_REFRESH_PROMISE_VIEW, EVENT_REFRESH_SETTLEMENT_VIEW, PROVIDER_KEY,
        RECEIPT_STATUS_MANUAL_REVIEW, RECEIPT_STATUS_REJECTED, RECEIPT_STATUS_VERIFIED,
        SETTLEMENT_CASE_FUNDED,
    },
    outbox::insert_outbox_message,
    repository::HappyRouteWriteRepository,
    state::{HappyRouteState, OutboxCommand, PaymentReceiptRecord},
    types::{CallbackContext, HappyRouteError, PaymentCallbackOutcome},
};

impl<'a> HappyRouteWriteRepository<'a> {
    pub(super) fn persist_payment_callback_result(
        &mut self,
        context: &CallbackContext,
        payment_id: &str,
        observed_amount: Money,
        verification: ReceiptVerification,
        normalized_observations: Vec<NormalizedObservation>,
    ) -> Result<PaymentCallbackOutcome, HappyRouteError> {
        let business_key = (PROVIDER_KEY.to_owned(), payment_id.to_owned());
        let receipt_status = receipt_status(&verification).to_owned();
        let verification_observations = verification_observations(&verification)?;
        if let Some(existing_payment_receipt_id) = self
            .store
            .payment_receipt_id_by_business_key
            .get(&business_key)
            .cloned()
        {
            let existing_receipt_status = self
                .store
                .payment_receipts_by_id
                .get(&existing_payment_receipt_id)
                .ok_or_else(|| {
                    HappyRouteError::Internal(
                        "payment receipt idempotency key points to missing receipt".to_owned(),
                    )
                })?
                .receipt_status
                .clone();

            if should_upgrade_existing_receipt(&existing_receipt_status, &receipt_status) {
                let existing_payment_receipt = self
                    .store
                    .payment_receipts_by_id
                    .get_mut(&existing_payment_receipt_id)
                    .ok_or_else(|| {
                        HappyRouteError::Internal(
                            "payment receipt idempotency key points to missing receipt".to_owned(),
                        )
                    })?;
                existing_payment_receipt.amount = observed_amount;
                existing_payment_receipt.receipt_status = receipt_status.clone();
                existing_payment_receipt.raw_callback_id = context.raw_callback_id.clone();

                append_normalized_observations(
                    self.store,
                    &context.settlement_case.settlement_case_id,
                    Some(&context.settlement_submission_id),
                    &normalized_observations,
                );
                append_normalized_observations(
                    self.store,
                    &context.settlement_case.settlement_case_id,
                    Some(&context.settlement_submission_id),
                    verification_observations,
                );

                let (case_status, ledger_journal_id, outbox_event_ids) =
                    apply_verified_receipt_side_effects(self.store, context);

                return Ok(PaymentCallbackOutcome {
                    payment_receipt_id: existing_payment_receipt_id,
                    raw_callback_id: context.raw_callback_id.clone(),
                    settlement_case_id: context.settlement_case.settlement_case_id.clone(),
                    promise_intent_id: context.promise_intent.promise_intent_id.clone(),
                    case_status,
                    receipt_status,
                    ledger_journal_id,
                    outbox_event_ids,
                    duplicate_receipt: false,
                });
            }

            let existing_payment_receipt = self
                .store
                .payment_receipts_by_id
                .get(&existing_payment_receipt_id)
                .ok_or_else(|| {
                    HappyRouteError::Internal(
                        "payment receipt idempotency key points to missing receipt".to_owned(),
                    )
                })?;
            let existing_settlement_case_id = existing_payment_receipt.settlement_case_id.clone();
            let existing_promise_intent_id = existing_payment_receipt.promise_intent_id.clone();
            let existing_receipt_status = existing_payment_receipt.receipt_status.clone();
            let existing_payment_receipt_id = existing_payment_receipt.payment_receipt_id.clone();
            append_normalized_observations(
                self.store,
                &context.settlement_case.settlement_case_id,
                Some(&context.settlement_submission_id),
                &normalized_observations,
            );
            append_normalized_observations(
                self.store,
                &context.settlement_case.settlement_case_id,
                Some(&context.settlement_submission_id),
                verification_observations,
            );

            return Ok(PaymentCallbackOutcome {
                payment_receipt_id: existing_payment_receipt_id,
                raw_callback_id: context.raw_callback_id.clone(),
                settlement_case_id: existing_settlement_case_id,
                promise_intent_id: existing_promise_intent_id,
                case_status: context.settlement_case.case_status.clone(),
                receipt_status: existing_receipt_status,
                ledger_journal_id: None,
                outbox_event_ids: Vec::new(),
                duplicate_receipt: true,
            });
        }

        let payment_receipt_id = Uuid::new_v4().to_string();
        let payment_receipt = PaymentReceiptRecord {
            payment_receipt_id: payment_receipt_id.clone(),
            provider_key: PROVIDER_KEY.to_owned(),
            external_payment_id: payment_id.to_owned(),
            settlement_case_id: context.settlement_case.settlement_case_id.clone(),
            promise_intent_id: context.promise_intent.promise_intent_id.clone(),
            amount: observed_amount,
            receipt_status: receipt_status.to_owned(),
            raw_callback_id: context.raw_callback_id.clone(),
            created_at: Utc::now(),
        };
        self.store
            .payment_receipt_id_by_business_key
            .insert(business_key, payment_receipt_id.clone());
        self.store
            .payment_receipts_by_id
            .insert(payment_receipt_id.clone(), payment_receipt);

        append_normalized_observations(
            self.store,
            &context.settlement_case.settlement_case_id,
            Some(&context.settlement_submission_id),
            &normalized_observations,
        );

        let mut ledger_journal_id = None;
        let mut outbox_event_ids = Vec::new();

        append_normalized_observations(
            self.store,
            &context.settlement_case.settlement_case_id,
            Some(&context.settlement_submission_id),
            verification_observations,
        );

        if receipt_status == RECEIPT_STATUS_VERIFIED {
            let funding_effects = apply_verified_receipt_side_effects(self.store, context);
            ledger_journal_id = funding_effects.1;
            outbox_event_ids = funding_effects.2;
        }

        let case_status = self
            .store
            .settlement_cases_by_id
            .get(&context.settlement_case.settlement_case_id)
            .map(|case| case.case_status.clone())
            .unwrap_or_else(|| context.settlement_case.case_status.clone());

        Ok(PaymentCallbackOutcome {
            payment_receipt_id,
            raw_callback_id: context.raw_callback_id.clone(),
            settlement_case_id: context.settlement_case.settlement_case_id.clone(),
            promise_intent_id: context.promise_intent.promise_intent_id.clone(),
            case_status,
            receipt_status: receipt_status.to_owned(),
            ledger_journal_id,
            outbox_event_ids,
            duplicate_receipt: false,
        })
    }
}

fn receipt_status(verification: &ReceiptVerification) -> &'static str {
    match verification {
        ReceiptVerification::Verified { .. } => RECEIPT_STATUS_VERIFIED,
        ReceiptVerification::Rejected { .. } => RECEIPT_STATUS_REJECTED,
        ReceiptVerification::NeedsManualReview { .. } => RECEIPT_STATUS_MANUAL_REVIEW,
        _ => RECEIPT_STATUS_MANUAL_REVIEW,
    }
}

fn verification_observations(
    verification: &ReceiptVerification,
) -> Result<&[NormalizedObservation], HappyRouteError> {
    match verification {
        ReceiptVerification::Verified { observations, .. }
        | ReceiptVerification::Rejected { observations, .. }
        | ReceiptVerification::NeedsManualReview { observations, .. } => Ok(observations),
        _ => Err(HappyRouteError::Internal(
            "receipt verification returned an unsupported non-exhaustive variant".to_owned(),
        )),
    }
}

fn should_upgrade_existing_receipt(existing_status: &str, next_status: &str) -> bool {
    existing_status != RECEIPT_STATUS_VERIFIED && next_status == RECEIPT_STATUS_VERIFIED
}

fn apply_verified_receipt_side_effects(
    store: &mut HappyRouteState,
    context: &CallbackContext,
) -> (String, Option<String>, Vec<String>) {
    let transitioned_to_funded = if let Some(settlement_case) = store
        .settlement_cases_by_id
        .get_mut(&context.settlement_case.settlement_case_id)
    {
        let should_transition = settlement_case.case_status != SETTLEMENT_CASE_FUNDED;
        if should_transition {
            settlement_case.case_status = SETTLEMENT_CASE_FUNDED.to_owned();
            settlement_case.updated_at = Utc::now();
        }
        should_transition
    } else {
        false
    };

    let mut ledger_journal_id = None;
    let mut outbox_event_ids = Vec::new();

    if transitioned_to_funded {
        ledger_journal_id = Some(append_receipt_recognition_ledger(
            store,
            &context.settlement_case,
            &context.promise_intent,
        ));
        outbox_event_ids.push(insert_outbox_message(
            store,
            "settlement_case",
            &context.settlement_case.settlement_case_id,
            EVENT_REFRESH_SETTLEMENT_VIEW,
            OutboxCommand::RefreshSettlementView {
                settlement_case_id: context.settlement_case.settlement_case_id.clone(),
            },
        ));
        outbox_event_ids.push(insert_outbox_message(
            store,
            "promise_intent",
            &context.promise_intent.promise_intent_id,
            EVENT_REFRESH_PROMISE_VIEW,
            OutboxCommand::RefreshPromiseView {
                promise_intent_id: context.promise_intent.promise_intent_id.clone(),
            },
        ));
    }

    let case_status = store
        .settlement_cases_by_id
        .get(&context.settlement_case.settlement_case_id)
        .map(|case| case.case_status.clone())
        .unwrap_or_else(|| context.settlement_case.case_status.clone());

    (case_status, ledger_journal_id, outbox_event_ids)
}
