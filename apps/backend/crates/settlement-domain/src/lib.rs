//! MUSUBI settlement-domain crate.
//! Owns pure settlement-facing domain concepts and adapter contracts.
//! Must not own provider implementations, DB persistence, or app/runtime wiring.
//! See `apps/backend/docs/package_boundaries.md`.

use std::{
    cmp::Ordering,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use musubi_core_domain::OrdinaryAccountId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PromiseId(String);

impl PromiseId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaymentReceiptId(String);

impl PaymentReceiptId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SettlementCaseId(String);

impl SettlementCaseId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SettlementIntentId(String);

impl SettlementIntentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SettlementSubmissionId(String);

impl SettlementSubmissionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObservationId(String);

impl ObservationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderSubmissionId(String);

impl ProviderSubmissionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InternalIdempotencyKey(String);

impl InternalIdempotencyKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderIdempotencyKey(String);

impl ProviderIdempotencyKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BackendKey(String);

impl BackendKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BackendVersion(String);

impl BackendVersion {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderRef(String);

impl ProviderRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderTxHash(String);

impl ProviderTxHash {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderCallbackId(String);

impl ProviderCallbackId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl Into<String>) -> Result<Self, CurrencyCodeError> {
        let value = value.into().trim().to_ascii_uppercase();

        if value.is_empty() {
            return Err(CurrencyCodeError::Empty);
        }

        if !value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
        {
            return Err(CurrencyCodeError::InvalidCharacter);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Money {
    currency: CurrencyCode,
    minor_units: i128,
    scale: u32,
}

impl Money {
    pub fn new(currency: CurrencyCode, minor_units: i128, scale: u32) -> Self {
        Self {
            currency,
            minor_units,
            scale,
        }
    }

    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }

    pub const fn minor_units(&self) -> i128 {
        self.minor_units
    }

    pub const fn scale(&self) -> u32 {
        self.scale
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, MoneyError> {
        self.ensure_compatible(other)?;

        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or(MoneyError::Overflow)?;

        Ok(Self::new(self.currency.clone(), minor_units, self.scale))
    }

    pub fn checked_sub(&self, other: &Self) -> Result<Self, MoneyError> {
        self.ensure_compatible(other)?;

        let minor_units = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or(MoneyError::Overflow)?;

        Ok(Self::new(self.currency.clone(), minor_units, self.scale))
    }

    pub fn checked_cmp(&self, other: &Self) -> Result<Ordering, MoneyError> {
        self.ensure_compatible(other)?;
        Ok(self.minor_units.cmp(&other.minor_units))
    }

    fn ensure_compatible(&self, other: &Self) -> Result<(), MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch {
                left: self.currency.clone(),
                right: other.currency.clone(),
            });
        }

        if self.scale != other.scale {
            return Err(MoneyError::ScaleMismatch {
                left: self.scale,
                right: other.scale,
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowStatus {
    Funded,
}

impl EscrowStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Funded => "Funded",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromiseParties {
    pub initiator_account_id: OrdinaryAccountId,
    pub counterparty_account_id: OrdinaryAccountId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendDescriptor {
    pub backend_key: BackendKey,
    pub backend_version: BackendVersion,
    pub provider_family: ProviderFamily,
    pub execution_mode: ExecutionMode,
    pub capabilities: BackendCapabilities,
}

impl BackendDescriptor {
    pub fn supports(&self, capability: SettlementCapability) -> bool {
        self.capabilities.supports(capability)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendCapabilities(Vec<SettlementCapability>);

impl BackendCapabilities {
    pub fn new(capabilities: Vec<SettlementCapability>) -> Self {
        Self(capabilities)
    }

    pub fn supports(&self, capability: SettlementCapability) -> bool {
        self.0.contains(&capability)
    }

    pub fn as_slice(&self) -> &[SettlementCapability] {
        &self.0
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderFamily {
    PiNetwork,
    Other(String),
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    Synchronous,
    Asynchronous,
    Hybrid,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettlementCapability {
    ReceiptVerify,
    HoldValue,
    ReleaseValue,
    RefundValue,
    CompensateValue,
    AllocateTreasury,
    AttestExecution,
    ReconcileStatus,
    NormalizeCallback,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationConfidence {
    CryptographicProof,
    ProviderConfirmed,
    HeuristicPending,
    Unknown,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizedObservationKind {
    ReceiptVerified,
    SubmissionAccepted,
    Pending,
    Finalized,
    Failed,
    Contradictory,
    NotFound,
    CallbackNormalized,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedObservation {
    pub observation_id: ObservationId,
    pub kind: NormalizedObservationKind,
    pub confidence: ObservationConfidence,
    pub observed_at: Option<SystemTime>,
    pub provider_ref: Option<ProviderRef>,
    pub provider_tx_hash: Option<ProviderTxHash>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderPayload(Vec<u8>);

impl ProviderPayload {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyReceiptCmd {
    pub receipt_id: PaymentReceiptId,
    pub raw_callback_ref: ProviderCallbackId,
    pub expected_amount: Option<Money>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitActionCmd {
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
    pub case_id: SettlementCaseId,
    pub submission_id: SettlementSubmissionId,
    pub provider_ref: Option<ProviderRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizeCallbackCmd {
    pub raw_callback_ref: ProviderCallbackId,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationRejectReason {
    SignatureInvalid,
    AmountMismatch,
    CurrencyMismatch,
    ProviderRejected,
    UnsupportedEvidence,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewReason {
    AmbiguousEvidence,
    ContradictoryEvidence,
    MissingEvidence,
    CapabilityMismatch,
    Timeout,
    UnknownProviderBehavior,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermanentFailureReason {
    CapabilityUnsupported,
    InvalidRequest,
    ProviderRejected,
    PolicyRefused,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContradictionReason {
    ConflictingObservations,
    MissingExpectedState,
    ProviderDisagreedWithReceipt,
    Unknown,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiptVerification {
    Verified {
        provider_ref: Option<ProviderRef>,
        observed_amount: Option<Money>,
        observed_at: Option<SystemTime>,
        observations: Vec<NormalizedObservation>,
    },
    Rejected {
        reason: VerificationRejectReason,
        observations: Vec<NormalizedObservation>,
    },
    NeedsManualReview {
        reason: ReviewReason,
        observations: Vec<NormalizedObservation>,
    },
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmissionResult {
    Accepted {
        provider_ref: Option<ProviderRef>,
        provider_submission_id: Option<ProviderSubmissionId>,
        provider_idempotency_key: ProviderIdempotencyKey,
        tx_hash: Option<ProviderTxHash>,
        observations: Vec<NormalizedObservation>,
    },
    Deferred {
        provider_idempotency_key: ProviderIdempotencyKey,
        retry_after: Option<Duration>,
        observations: Vec<NormalizedObservation>,
    },
    RejectedPermanent {
        reason: PermanentFailureReason,
        observations: Vec<NormalizedObservation>,
    },
    NeedsManualReview {
        reason: ReviewReason,
        observations: Vec<NormalizedObservation>,
    },
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconcileResult {
    Pending {
        observations: Vec<NormalizedObservation>,
    },
    Finalized {
        observations: Vec<NormalizedObservation>,
    },
    Contradictory {
        observations: Vec<NormalizedObservation>,
        reason: ContradictionReason,
    },
    NotFound {
        observations: Vec<NormalizedObservation>,
    },
    NeedsManualReview {
        reason: ReviewReason,
        observations: Vec<NormalizedObservation>,
    },
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CurrencyCodeError {
    Empty,
    InvalidCharacter,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoneyError {
    CurrencyMismatch {
        left: CurrencyCode,
        right: CurrencyCode,
    },
    ScaleMismatch {
        left: u32,
        right: u32,
    },
    Overflow,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    CapabilityUnsupported {
        backend_key: BackendKey,
        capability: SettlementCapability,
    },
    InvalidProviderPayload,
    InvalidProviderResponse,
    Timeout,
    TemporarilyUnavailable,
    IdempotencyMappingFailed,
    ObservationNormalizationFailed,
}

#[async_trait]
pub trait SettlementBackend: Send + Sync {
    fn descriptor(&self) -> &BackendDescriptor;

    fn supports(&self, capability: SettlementCapability) -> bool {
        self.descriptor().supports(capability)
    }

    fn provider_idempotency_key(
        &self,
        internal_idempotency_key: &InternalIdempotencyKey,
    ) -> Result<ProviderIdempotencyKey, BackendError>;

    async fn verify_receipt(
        &self,
        cmd: VerifyReceiptCmd,
    ) -> Result<ReceiptVerification, BackendError>;

    async fn submit_action(&self, cmd: SubmitActionCmd) -> Result<SubmissionResult, BackendError>;

    async fn reconcile_submission(
        &self,
        cmd: ReconcileSubmissionCmd,
    ) -> Result<ReconcileResult, BackendError>;

    async fn normalize_callback(
        &self,
        cmd: NormalizeCallbackCmd,
    ) -> Result<Vec<NormalizedObservation>, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn currency(code: &str) -> CurrencyCode {
        CurrencyCode::new(code).expect("currency code must be valid in tests")
    }

    #[test]
    fn checked_add_rejects_currency_mismatch() {
        let left = Money::new(currency("PI"), 1000, 3);
        let right = Money::new(currency("JPY"), 1000, 0);

        let result = left.checked_add(&right);

        assert_eq!(
            result,
            Err(MoneyError::CurrencyMismatch {
                left: currency("PI"),
                right: currency("JPY"),
            })
        );
    }

    #[test]
    fn checked_sub_rejects_scale_mismatch() {
        let left = Money::new(currency("PI"), 1000, 3);
        let right = Money::new(currency("PI"), 1000, 6);

        let result = left.checked_sub(&right);

        assert_eq!(result, Err(MoneyError::ScaleMismatch { left: 3, right: 6 }));
    }

    #[test]
    fn descriptor_supports_explicit_capabilities() {
        let descriptor = BackendDescriptor {
            backend_key: BackendKey::new("pi"),
            backend_version: BackendVersion::new("2026-04"),
            provider_family: ProviderFamily::PiNetwork,
            execution_mode: ExecutionMode::Hybrid,
            capabilities: BackendCapabilities::new(vec![
                SettlementCapability::ReceiptVerify,
                SettlementCapability::HoldValue,
            ]),
        };

        assert!(descriptor.supports(SettlementCapability::ReceiptVerify));
        assert!(!descriptor.supports(SettlementCapability::RefundValue));
    }
}
