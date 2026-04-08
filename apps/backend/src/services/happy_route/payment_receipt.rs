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
    state::{OutboxCommand, PaymentReceiptRecord},
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
        if let Some(existing_payment_receipt_id) = self
            .store
            .payment_receipt_id_by_business_key
            .get(&business_key)
            .cloned()
        {
            let existing_payment_receipt = self
                .store
                .payment_receipts_by_id
                .get(&existing_payment_receipt_id)
                .ok_or_else(|| {
                    HappyRouteError::Internal(
                        "payment receipt idempotency key points to missing receipt".to_owned(),
                    )
                })?;

            return Ok(PaymentCallbackOutcome {
                payment_receipt_id: existing_payment_receipt.payment_receipt_id.clone(),
                raw_callback_id: context.raw_callback_id.clone(),
                settlement_case_id: existing_payment_receipt.settlement_case_id.clone(),
                promise_intent_id: existing_payment_receipt.promise_intent_id.clone(),
                case_status: context.settlement_case.case_status.clone(),
                receipt_status: existing_payment_receipt.receipt_status.clone(),
                ledger_journal_id: None,
                outbox_event_ids: Vec::new(),
                duplicate_receipt: true,
            });
        }

        let payment_receipt_id = Uuid::new_v4().to_string();
        let receipt_status = receipt_status(&verification);
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

        match verification {
            ReceiptVerification::Verified { observations, .. } => {
                append_normalized_observations(
                    self.store,
                    &context.settlement_case.settlement_case_id,
                    Some(&context.settlement_submission_id),
                    &observations,
                );

                if let Some(settlement_case) = self
                    .store
                    .settlement_cases_by_id
                    .get_mut(&context.settlement_case.settlement_case_id)
                {
                    settlement_case.case_status = SETTLEMENT_CASE_FUNDED.to_owned();
                    settlement_case.updated_at = Utc::now();
                }

                let journal_entry_id = append_receipt_recognition_ledger(
                    self.store,
                    &context.settlement_case,
                    &context.promise_intent,
                );
                ledger_journal_id = Some(journal_entry_id);
                outbox_event_ids.push(insert_outbox_message(
                    self.store,
                    "settlement_case",
                    &context.settlement_case.settlement_case_id,
                    EVENT_REFRESH_SETTLEMENT_VIEW,
                    OutboxCommand::RefreshSettlementView {
                        settlement_case_id: context.settlement_case.settlement_case_id.clone(),
                    },
                ));
                outbox_event_ids.push(insert_outbox_message(
                    self.store,
                    "promise_intent",
                    &context.promise_intent.promise_intent_id,
                    EVENT_REFRESH_PROMISE_VIEW,
                    OutboxCommand::RefreshPromiseView {
                        promise_intent_id: context.promise_intent.promise_intent_id.clone(),
                    },
                ));
            }
            ReceiptVerification::Rejected { observations, .. }
            | ReceiptVerification::NeedsManualReview { observations, .. } => {
                append_normalized_observations(
                    self.store,
                    &context.settlement_case.settlement_case_id,
                    Some(&context.settlement_submission_id),
                    &observations,
                );
            }
            _ => {
                return Err(HappyRouteError::Internal(
                    "receipt verification returned an unsupported non-exhaustive variant"
                        .to_owned(),
                ));
            }
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
