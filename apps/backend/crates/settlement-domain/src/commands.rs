use crate::{
    BackendPin, CurrencyCode, InternalIdempotencyKey, Money, PaymentReceiptId, ProviderCallbackId,
    ProviderPayload, ProviderRef, SettlementCapability, SettlementCaseId, SettlementIntentId,
    SettlementSubmissionId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyReceiptCmd {
    pub backend: BackendPin,
    pub receipt_id: PaymentReceiptId,
    pub raw_callback_ref: ProviderCallbackId,
    pub expected_amount: Option<Money>,
    pub expected_currency: Option<CurrencyCode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitActionCmd {
    pub backend: BackendPin,
    pub case_id: SettlementCaseId,
    pub intent_id: SettlementIntentId,
    pub submission_id: SettlementSubmissionId,
    pub internal_idempotency_key: InternalIdempotencyKey,
    pub capability: SettlementCapability,
    pub amount: Option<Money>,
    pub provider_payload: ProviderPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileSubmissionCmd {
    pub backend: BackendPin,
    pub case_id: SettlementCaseId,
    pub submission_id: SettlementSubmissionId,
    pub provider_ref: Option<ProviderRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizeCallbackCmd {
    pub backend: BackendPin,
    pub raw_callback_ref: ProviderCallbackId,
}
