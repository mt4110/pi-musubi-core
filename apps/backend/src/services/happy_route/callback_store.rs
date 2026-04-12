use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use musubi_settlement_domain::Money;

use super::{
    backend::callback_dedupe_key,
    constants::PROVIDER_KEY,
    repository::HappyRouteWriteRepository,
    state::RawProviderCallbackRecord,
    types::{
        CallbackContext, HappyRouteError, ParsedPaymentCallback, PaymentCallbackInput,
        RawPaymentCallbackFields,
    },
};

impl<'a> HappyRouteWriteRepository<'a> {
    pub(super) fn record_raw_callback(
        &mut self,
        input: &PaymentCallbackInput,
        raw_fields: Option<&RawPaymentCallbackFields>,
        raw_callback_id: &str,
        received_at: DateTime<Utc>,
    ) -> bool {
        let dedupe_key = callback_dedupe_key(&input.raw_body_bytes);
        let replay_of_raw_callback_id = self
            .store
            .raw_provider_callback_id_by_dedupe_key
            .get(&dedupe_key)
            .cloned();
        let duplicate_callback = replay_of_raw_callback_id.is_some();

        let raw_callback = RawProviderCallbackRecord {
            raw_callback_id: raw_callback_id.to_owned(),
            provider_name: PROVIDER_KEY.to_owned(),
            dedupe_key: dedupe_key.clone(),
            replay_of_raw_callback_id,
            raw_body_bytes: input.raw_body_bytes.clone(),
            raw_body: std::str::from_utf8(&input.raw_body_bytes)
                .map(str::to_owned)
                .unwrap_or_else(|_| "[non-utf8 body; see raw_body_bytes]".to_owned()),
            redacted_headers: input.redacted_headers.clone(),
            signature_valid: None,
            provider_submission_id: raw_fields
                .and_then(|payload| payload.provider_submission_id.clone()),
            provider_ref: None,
            payer_pi_uid: raw_fields.and_then(|payload| payload.payer_pi_uid.clone()),
            amount_minor_units: raw_fields.and_then(|payload| payload.amount_minor_units),
            currency_code: raw_fields.and_then(|payload| payload.currency_code.clone()),
            amount: None,
            txid: raw_fields.and_then(|payload| payload.txid.clone()),
            callback_status: raw_fields.and_then(|payload| payload.callback_status.clone()),
            received_at,
        };
        self.store
            .raw_provider_callback_id_by_dedupe_key
            .entry(dedupe_key)
            .or_insert_with(|| raw_callback_id.to_owned());
        self.store
            .raw_provider_callbacks_by_id
            .insert(raw_callback_id.to_owned(), raw_callback);

        duplicate_callback
    }

    pub(super) fn attach_callback_amount(
        &mut self,
        raw_callback_id: &str,
        observed_amount: &Money,
    ) {
        if let Some(raw_callback) = self
            .store
            .raw_provider_callbacks_by_id
            .get_mut(raw_callback_id)
        {
            raw_callback.amount = Some(observed_amount.clone());
        }
    }

    pub(super) fn load_callback_context(
        &mut self,
        parsed: &ParsedPaymentCallback,
        observed_amount: &Money,
        raw_callback_id: &str,
        duplicate_callback: bool,
    ) -> Result<CallbackContext, HappyRouteError> {
        let settlement_submission_id = self
            .store
            .settlement_submission_id_by_provider_submission_id
            .get(&parsed.provider_submission_id)
            .cloned()
            .ok_or_else(|| {
                HappyRouteError::ProviderCallbackMappingDeferred(format!(
                    "provider submission mapping is not ready for provider_submission_id {}",
                    parsed.provider_submission_id
                ))
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
        if initiator_account.pi_uid != parsed.payer_pi_uid {
            return Err(HappyRouteError::BadRequest(
                "payer_pi_uid does not match the Promise initiator".to_owned(),
            ));
        }

        if let Some(raw_callback) = self
            .store
            .raw_provider_callbacks_by_id
            .get_mut(raw_callback_id)
        {
            raw_callback.provider_ref = submission.provider_ref.clone();
        }

        Ok(CallbackContext {
            raw_callback_id: raw_callback_id.to_owned(),
            duplicate_callback,
            provider_submission_id: parsed.provider_submission_id.clone(),
            settlement_case,
            settlement_submission_id,
            promise_intent,
        })
    }
}
