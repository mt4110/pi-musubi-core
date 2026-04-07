use std::time::{Duration, SystemTime};

use crate::{
    ContradictionReason, Money, NormalizedObservation, PermanentFailureReason,
    ProviderIdempotencyKey, ProviderRef, ProviderSubmissionId, ProviderTxHash, ReviewReason,
    VerificationRejectReason,
};

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
