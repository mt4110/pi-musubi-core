use std::time::SystemTime;

use crate::{ObservationId, ProviderRef, ProviderTxHash};

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
