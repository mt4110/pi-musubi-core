//! MUSUBI settlement-domain crate.
//! Owns pure settlement-facing domain concepts and adapter contracts.
//! Must not own provider implementations, DB persistence, or app/runtime wiring.
//! See `apps/backend/docs/package_boundaries.md`.

mod backend;
mod commands;
mod descriptor;
mod errors;
mod ids;
mod money;
mod observation;
mod payload;
mod results;
mod state;

pub use backend::{BackendKey, BackendPin, BackendVersion, SettlementBackend};
pub use commands::{
    NormalizeCallbackCmd, ReconcileSubmissionCmd, SubmitActionCmd, VerifyReceiptCmd,
    VerifyReceiptExpectation,
};
pub use descriptor::{
    BackendCapabilities, BackendDescriptor, ExecutionMode, ProviderFamily, SettlementCapability,
};
pub use errors::{
    BackendError, ContradictionReason, PermanentFailureReason, ReviewReason,
    VerificationRejectReason,
};
pub use ids::{
    EscrowStatus, InternalIdempotencyKey, ObservationId, PaymentReceiptId, PromiseId,
    PromiseParties, ProviderCallbackId, ProviderIdempotencyKey, ProviderRef, ProviderSubmissionId,
    ProviderTxHash, SettlementCaseId, SettlementIntentId, SettlementSubmissionId,
};
pub use money::{CurrencyCode, CurrencyCodeError, Money, MoneyError};
pub use observation::{NormalizedObservation, NormalizedObservationKind, ObservationConfidence};
pub use payload::{
    ProviderPayload, ProviderPayloadField, ProviderPayloadSchema, ProviderPayloadValue,
};
pub use results::{ReceiptVerification, ReconcileResult, SubmissionResult};
pub use state::{
    SettlementOverlay, SettlementPrimaryPhase, SettlementResolutionKind, SettlementState,
};
