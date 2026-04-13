use crate::{BackendKey, BackendPin, SettlementCapability};

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
pub enum BackendError {
    CapabilityUnsupported {
        backend_key: BackendKey,
        capability: SettlementCapability,
    },
    BackendPinMismatch {
        requested: BackendPin,
        available: BackendPin,
    },
    InvalidConfiguration(String),
    InvalidProviderPayload,
    InvalidProviderResponse,
    Timeout,
    TemporarilyUnavailable,
    IdempotencyMappingFailed,
    ObservationNormalizationFailed,
}
