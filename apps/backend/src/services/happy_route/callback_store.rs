use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use musubi_settlement_domain::Money;

use super::{
    repository::HappyRouteWriteRepository,
    state::RawProviderCallbackRecord,
    types::{CallbackContext, HappyRouteError, PaymentCallbackInput},
};

impl<'a> HappyRouteWriteRepository<'a> {
    pub(super) fn record_raw_callback_and_load_context(
        &mut self,
        input: &PaymentCallbackInput,
        observed_amount: &Money,
        raw_callback_id: &str,
        received_at: DateTime<Utc>,
    ) -> Result<CallbackContext, HappyRouteError> {
        let settlement_submission_id = self
            .store
            .settlement_submission_id_by_provider_submission_id
            .get(&input.payment_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::NotFound(
                    "payment_id was not recognized by settlement submissions".to_owned(),
                )
            })?;
        let submission = self
            .store
            .settlement_submissions_by_id
            .get(&settlement_submission_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::Internal(
                    "provider submission mapping points to missing submission".to_owned(),
                )
            })?;
        let settlement_case = self
            .store
            .settlement_cases_by_id
            .get(&submission.settlement_case_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::Internal(
                    "settlement submission points to missing settlement case".to_owned(),
                )
            })?;
        let promise_intent = self
            .store
            .promise_intents_by_id
            .get(&settlement_case.promise_intent_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::Internal(
                    "settlement case points to missing promise intent".to_owned(),
                )
            })?;
        if promise_intent
            .deposit_amount
            .checked_cmp(observed_amount)
            .map_err(|_| {
                HappyRouteError::BadRequest(
                    "callback amount is incompatible with promise amount".to_owned(),
                )
            })?
            != Ordering::Equal
        {
            return Err(HappyRouteError::BadRequest(
                "callback amount does not match the bounded Promise deposit".to_owned(),
            ));
        }

        let initiator_account = self
            .store
            .accounts_by_id
            .get(&promise_intent.initiator_account_id)
            .ok_or_else(|| {
                HappyRouteError::Internal(
                    "promise initiator account is missing from state".to_owned(),
                )
            })?;
        if initiator_account.pi_uid != input.payer_pi_uid {
            return Err(HappyRouteError::BadRequest(
                "payer_pi_uid does not match the Promise initiator".to_owned(),
            ));
        }

        let raw_callback = RawProviderCallbackRecord {
            raw_callback_id: raw_callback_id.to_owned(),
            payment_id: input.payment_id.clone(),
            payer_pi_uid: input.payer_pi_uid.clone(),
            amount: observed_amount.clone(),
            txid: input.txid.clone(),
            callback_status: input.callback_status.clone(),
            received_at,
        };
        self.store
            .raw_provider_callbacks_by_id
            .insert(raw_callback_id.to_owned(), raw_callback);

        Ok(CallbackContext {
            raw_callback_id: raw_callback_id.to_owned(),
            settlement_case,
            settlement_submission_id,
            promise_intent,
        })
    }
}
