use chrono::{DateTime, Utc};

#[derive(Debug)]
pub enum PromiseCompletionWriterFactPersistenceError {
    BadRequest(String),
    IdempotencyConflict {
        message: String,
        existing_writer_fact_id: String,
    },
    Database {
        message: String,
        code: Option<String>,
        constraint: Option<String>,
        retryable: bool,
    },
    Internal(String),
}

impl PromiseCompletionWriterFactPersistenceError {
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::IdempotencyConflict { message, .. }
            | Self::Database { message, .. }
            | Self::Internal(message) => message,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromiseCompletionWriterFactReplayStatus {
    Inserted,
    ReplayedIdentical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromiseCompletionWriterFactFamily {
    SourceRouteCandidate,
    CompletionStateTransition,
    CompletionOutcomeReference,
    CorrectionOrSupersession,
    AccessAuditRetentionSupport,
}

impl PromiseCompletionWriterFactFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceRouteCandidate => "source_route_candidate",
            Self::CompletionStateTransition => "completion_state_transition",
            Self::CompletionOutcomeReference => "completion_outcome_reference",
            Self::CorrectionOrSupersession => "correction_or_supersession",
            Self::AccessAuditRetentionSupport => "access_audit_retention_support",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromiseCompletionSourceRouteClass {
    MutualAccountableCompletionAcknowledgement,
    GovernedReviewCompletion,
    Forbidden(PromiseCompletionForbiddenSourceRouteClass),
}

impl PromiseCompletionSourceRouteClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MutualAccountableCompletionAcknowledgement => {
                "mutual_accountable_completion_acknowledgement"
            }
            Self::GovernedReviewCompletion => "governed_review_completion",
            Self::Forbidden(route) => route.as_str(),
        }
    }

    pub fn is_allowed(self) -> bool {
        !matches!(self, Self::Forbidden(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromiseCompletionForbiddenSourceRouteClass {
    ProofOnlyCompletion,
    SettlementOnlyCompletion,
    PaymentOnlyCompletion,
    ProviderCallbackOnlyCompletion,
    OperatorNoteOnlyCompletion,
    ProjectionOnlyCompletion,
    ModelOutputOnlyCompletion,
    VenueStaffJudgmentOnlyCompletion,
    ClientStateOnlyCompletion,
    SupportStatusCompletion,
    ImplementationConvenienceCompletion,
    SilenceBasedCompletion,
    PopularityBasedCompletion,
}

impl PromiseCompletionForbiddenSourceRouteClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProofOnlyCompletion => "proof_only_completion",
            Self::SettlementOnlyCompletion => "settlement_only_completion",
            Self::PaymentOnlyCompletion => "payment_only_completion",
            Self::ProviderCallbackOnlyCompletion => "provider_callback_only_completion",
            Self::OperatorNoteOnlyCompletion => "operator_note_only_completion",
            Self::ProjectionOnlyCompletion => "projection_only_completion",
            Self::ModelOutputOnlyCompletion => "model_output_only_completion",
            Self::VenueStaffJudgmentOnlyCompletion => "venue_staff_judgment_only_completion",
            Self::ClientStateOnlyCompletion => "client_state_only_completion",
            Self::SupportStatusCompletion => "support_status_completion",
            Self::ImplementationConvenienceCompletion => "implementation_convenience_completion",
            Self::SilenceBasedCompletion => "silence_based_completion",
            Self::PopularityBasedCompletion => "popularity_based_completion",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromiseCompletionStateClass {
    CompletionUnavailable,
    CompletionPendingMutualAcknowledgement,
    CompletionReviewRequired,
    CompletionUnderGovernedReview,
    CompletionAccepted,
    CompletionRejected,
    CompletionExpired,
    CompletionCorrectedOrSuperseded,
    CompletionClosed,
}

impl PromiseCompletionStateClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompletionUnavailable => "completion_unavailable",
            Self::CompletionPendingMutualAcknowledgement => {
                "completion_pending_mutual_acknowledgement"
            }
            Self::CompletionReviewRequired => "completion_review_required",
            Self::CompletionUnderGovernedReview => "completion_under_governed_review",
            Self::CompletionAccepted => "completion_accepted",
            Self::CompletionRejected => "completion_rejected",
            Self::CompletionExpired => "completion_expired",
            Self::CompletionCorrectedOrSuperseded => "completion_corrected_or_superseded",
            Self::CompletionClosed => "completion_closed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromiseCompletionProjectionNonAuthorityPosture {
    ProjectionNonAuthoritative,
    ProjectionAuthority,
}

impl PromiseCompletionProjectionNonAuthorityPosture {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectionNonAuthoritative => "projection_non_authoritative",
            Self::ProjectionAuthority => "projection_authority",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromiseCompletionAuthorityPosture {
    WriterTruthOnly,
    ProjectionOnly,
}

impl PromiseCompletionAuthorityPosture {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WriterTruthOnly => "writer_truth_only",
            Self::ProjectionOnly => "projection_only",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordPromiseCompletionWriterFactInput {
    pub fact: ProposedPromiseCompletionWriterFact,
}

#[derive(Clone, Debug)]
pub struct ProposedPromiseCompletionWriterFact {
    pub promise_reference: Option<String>,
    pub realm_id: Option<String>,
    pub fact_family: PromiseCompletionWriterFactFamily,
    pub source_route_class: PromiseCompletionSourceRouteClass,
    pub previous_completion_state_class: Option<PromiseCompletionStateClass>,
    pub completion_state_class: PromiseCompletionStateClass,
    pub completed_reference_eligible: bool,
    pub promise_terms_reference: Option<String>,
    pub participant_set_reference: Option<String>,
    pub ordinary_participant_acknowledgement_reference: Option<String>,
    pub governed_review_reference: Option<String>,
    pub review_authority_reference: Option<String>,
    pub proof_eligibility_reference: Option<String>,
    pub proof_evidence_writer_fact_reference: Option<String>,
    pub consent_at_formation_reference: Option<String>,
    pub consent_at_resolution_reference: Option<String>,
    pub block_withdrawal_state_reference: Option<String>,
    pub age_assurance_state_reference: Option<String>,
    pub legal_hold_intersection_reference: Option<String>,
    pub critical_harm_case_reference: Option<String>,
    pub account_lifecycle_reference: Option<String>,
    pub anti_abuse_continuity_reference: Option<String>,
    pub safety_case_reference: Option<String>,
    pub reason_code_class: Option<String>,
    pub evidence_level_reference: Option<String>,
    pub correction_or_supersession_reference: Option<String>,
    pub prior_writer_fact_id: Option<String>,
    pub policy_version: Option<i32>,
    pub fact_idempotency_key: Option<String>,
    pub retention_class_reference: Option<String>,
    pub access_audit_reference: Option<String>,
    pub projection_non_authority_posture: Option<PromiseCompletionProjectionNonAuthorityPosture>,
    pub authority_posture: Option<PromiseCompletionAuthorityPosture>,
}

#[derive(Clone, Debug)]
pub struct PromiseCompletionWriterFactSnapshot {
    pub writer_fact_id: String,
    pub promise_reference: String,
    pub realm_id: String,
    pub fact_family: String,
    pub source_route_class: String,
    pub completion_state_class: String,
    pub completed_reference_eligible: bool,
    pub request_payload_hash: String,
    pub decision_payload_hash: String,
    pub replay_status: PromiseCompletionWriterFactReplayStatus,
    pub created_at: DateTime<Utc>,
}
