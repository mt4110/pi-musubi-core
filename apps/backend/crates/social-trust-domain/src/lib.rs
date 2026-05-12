//! MUSUBI social-trust-domain crate.
//! Owns the pure Social Trust intake / no-authority decision contract.
//! Must not own DB persistence, projections, HTTP/API wiring, Relationship Depth,
//! proof, settlement, discovery, recommendation, or runtime orchestration.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedSocialTrustMutationAttempt {
    pub source_category: SocialTrustSourceCategory,
    pub writer_source_reference: Option<WriterSourceReference>,
    pub reason_fact: Option<ReasonFactReference>,
    pub idempotency_posture: Option<DurableIdempotencyPosture>,
    pub evidence_posture: Option<EvidencePosture>,
    pub reviewability_posture: Option<ReviewabilityPosture>,
    pub retention_posture: Option<RetentionPosture>,
    pub authority_posture: SocialTrustAuthorityPosture,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialTrustSourceCategory {
    /// A placeholder for a writer-owned source category that a later accepted
    /// implementation boundary must name before persistence is wired. This is
    /// not a Social Trust source taxonomy or a runtime allowlist.
    WriterSourceCandidate,
    Forbidden(ForbiddenSocialTrustSourceCategory),
    Unknown,
}

impl SocialTrustSourceCategory {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WriterSourceCandidate => "writer_source_candidate",
            Self::Forbidden(source) => source.as_str(),
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForbiddenSocialTrustSourceCategory {
    ProjectionState,
    AnalyticsState,
    ModelOutput,
    ObservabilityState,
    ClientState,
    FrontendState,
    PaymentAmount,
    PaymentFrequency,
    SupportAmountOrStatus,
    TokenHoldings,
    Popularity,
    FollowerCount,
    ReplySpeed,
    DwellTime,
    Tenure,
    RomanticDesirability,
    Engagement,
    EngagementTelemetry,
    RecommendationState,
    DiscoveryState,
    DiscoveryRanking,
    RelationshipDepth,
    RoomProjection,
    OperatorNotes,
    SupportTickets,
    IssueComments,
    AntiAbuseMarkerExistence,
    AgeAssurancePosture,
    ProofCallbackAlone,
    VendorCallbackAlone,
    ControlledExceptionalAccountActivity,
    ImplementationConvenience,
}

impl ForbiddenSocialTrustSourceCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectionState => "projection_state",
            Self::AnalyticsState => "analytics_state",
            Self::ModelOutput => "model_output",
            Self::ObservabilityState => "observability_state",
            Self::ClientState => "client_state",
            Self::FrontendState => "frontend_state",
            Self::PaymentAmount => "payment_amount",
            Self::PaymentFrequency => "payment_frequency",
            Self::SupportAmountOrStatus => "support_amount_or_status",
            Self::TokenHoldings => "token_holdings",
            Self::Popularity => "popularity",
            Self::FollowerCount => "follower_count",
            Self::ReplySpeed => "reply_speed",
            Self::DwellTime => "dwell_time",
            Self::Tenure => "tenure",
            Self::RomanticDesirability => "romantic_desirability",
            Self::Engagement => "engagement",
            Self::EngagementTelemetry => "engagement_telemetry",
            Self::RecommendationState => "recommendation_state",
            Self::DiscoveryState => "discovery_state",
            Self::DiscoveryRanking => "discovery_ranking",
            Self::RelationshipDepth => "relationship_depth",
            Self::RoomProjection => "room_projection",
            Self::OperatorNotes => "operator_notes",
            Self::SupportTickets => "support_tickets",
            Self::IssueComments => "issue_comments",
            Self::AntiAbuseMarkerExistence => "anti_abuse_marker_existence",
            Self::AgeAssurancePosture => "age_assurance_posture",
            Self::ProofCallbackAlone => "proof_callback_alone",
            Self::VendorCallbackAlone => "vendor_callback_alone",
            Self::ControlledExceptionalAccountActivity => "controlled_exceptional_account_activity",
            Self::ImplementationConvenience => "implementation_convenience",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableIdempotencyPosture {
    DurableDedupeKey(SocialTrustMutationAttemptIdempotencyKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidencePosture {
    Bounded,
}

impl EvidencePosture {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Bounded => "bounded",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewabilityPosture {
    Reviewable,
}

impl ReviewabilityPosture {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Reviewable => "reviewable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetentionPosture {
    Classified(RetentionClassReference),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocialTrustAuthorityPosture {
    WriterTruthOnly,
    ProjectionOnly,
}

impl SocialTrustAuthorityPosture {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WriterTruthOnly => "writer_truth_only",
            Self::ProjectionOnly => "projection_only",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialTrustIntakeDecision {
    Reject(SocialTrustIntakeRejection),
    /// The attempt satisfied this pure intake contract, but Social Trust has
    /// not been mutated and no writer fact has been persisted by this crate.
    CandidateForWriterPersistence,
}

impl SocialTrustIntakeDecision {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Reject(_) => "rejected",
            Self::CandidateForWriterPersistence => "candidate_for_writer_persistence",
        }
    }

    pub const fn rejection_reason_code(&self) -> Option<&'static str> {
        match self {
            Self::Reject(rejection) => Some(rejection.as_str()),
            Self::CandidateForWriterPersistence => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialTrustIntakeRejection {
    ForbiddenSource {
        source: ForbiddenSocialTrustSourceCategory,
    },
    UnknownSourceCategory,
    ProjectionOnlyAuthority,
    MissingWriterSourceReference,
    MissingReasonFact,
    MissingIdempotencyPosture,
    MissingEvidencePosture,
    MissingReviewabilityPosture,
    MissingRetentionPosture,
}

impl SocialTrustIntakeRejection {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ForbiddenSource { .. } => "forbidden_source",
            Self::UnknownSourceCategory => "unknown_source_category",
            Self::ProjectionOnlyAuthority => "projection_only_authority",
            Self::MissingWriterSourceReference => "missing_writer_source_reference",
            Self::MissingReasonFact => "missing_reason_fact",
            Self::MissingIdempotencyPosture => "missing_idempotency_posture",
            Self::MissingEvidencePosture => "missing_evidence_posture",
            Self::MissingReviewabilityPosture => "missing_reviewability_posture",
            Self::MissingRetentionPosture => "missing_retention_posture",
        }
    }
}

#[must_use]
pub fn decide_social_trust_intake(
    attempt: &ProposedSocialTrustMutationAttempt,
) -> SocialTrustIntakeDecision {
    match &attempt.source_category {
        SocialTrustSourceCategory::Forbidden(source) => {
            return SocialTrustIntakeDecision::Reject(
                SocialTrustIntakeRejection::ForbiddenSource { source: *source },
            );
        }
        SocialTrustSourceCategory::Unknown => {
            return SocialTrustIntakeDecision::Reject(
                SocialTrustIntakeRejection::UnknownSourceCategory,
            );
        }
        SocialTrustSourceCategory::WriterSourceCandidate => {}
    }

    if attempt.authority_posture == SocialTrustAuthorityPosture::ProjectionOnly {
        return SocialTrustIntakeDecision::Reject(
            SocialTrustIntakeRejection::ProjectionOnlyAuthority,
        );
    }

    if !attempt
        .writer_source_reference
        .as_ref()
        .is_some_and(WriterSourceReference::is_present)
    {
        return SocialTrustIntakeDecision::Reject(
            SocialTrustIntakeRejection::MissingWriterSourceReference,
        );
    }

    if !attempt
        .reason_fact
        .as_ref()
        .is_some_and(ReasonFactReference::is_present)
    {
        return SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingReasonFact);
    }

    if !attempt
        .idempotency_posture
        .as_ref()
        .is_some_and(DurableIdempotencyPosture::is_present)
    {
        return SocialTrustIntakeDecision::Reject(
            SocialTrustIntakeRejection::MissingIdempotencyPosture,
        );
    }

    if attempt.evidence_posture.is_none() {
        return SocialTrustIntakeDecision::Reject(
            SocialTrustIntakeRejection::MissingEvidencePosture,
        );
    }

    if attempt.reviewability_posture.is_none() {
        return SocialTrustIntakeDecision::Reject(
            SocialTrustIntakeRejection::MissingReviewabilityPosture,
        );
    }

    if !attempt
        .retention_posture
        .as_ref()
        .is_some_and(RetentionPosture::is_present)
    {
        return SocialTrustIntakeDecision::Reject(
            SocialTrustIntakeRejection::MissingRetentionPosture,
        );
    }

    SocialTrustIntakeDecision::CandidateForWriterPersistence
}

macro_rules! string_ref {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }

            pub fn is_present(&self) -> bool {
                !self.0.trim().is_empty()
            }
        }
    };
}

string_ref!(WriterSourceReference);
string_ref!(ReasonFactReference);
string_ref!(SocialTrustMutationAttemptIdempotencyKey);
string_ref!(RetentionClassReference);

impl DurableIdempotencyPosture {
    pub fn is_present(&self) -> bool {
        match self {
            Self::DurableDedupeKey(key) => key.is_present(),
        }
    }
}

impl RetentionPosture {
    pub fn is_present(&self) -> bool {
        match self {
            Self::Classified(reference) => reference.is_present(),
        }
    }

    pub fn class_reference(&self) -> &RetentionClassReference {
        match self {
            Self::Classified(reference) => reference,
        }
    }
}
