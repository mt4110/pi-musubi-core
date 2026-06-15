//! MUSUBI social-trust-domain crate.
//! Owns pure Social Trust intake and categorical writer-fact decision contracts.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedC2BoundedPromiseReliabilityMutationFact {
    pub source_fact: C2BoundedPromiseReliabilitySourceFactCandidate,
    pub requested_mutation_fact: C2BoundedPromiseReliabilityMutationFactCandidate,
    pub writer_source_reference: Option<WriterSourceReference>,
    pub promise_reference: Option<PromiseReference>,
    pub promise_terms_reference: Option<PromiseTermsReference>,
    pub consent_at_formation_reference: Option<ConsentStateReference>,
    pub consent_at_resolution_reference: Option<ConsentStateReference>,
    pub block_withdrawal_state_reference: Option<BlockWithdrawalStateReference>,
    pub age_assurance_state_reference: Option<AgeAssuranceStateReference>,
    pub legal_hold_intersection_reference: Option<LegalHoldIntersectionReference>,
    pub critical_harm_case_reference: Option<CriticalHarmCaseReference>,
    pub account_lifecycle_reference: Option<AccountLifecycleReference>,
    pub anti_abuse_continuity_reference: Option<AntiAbuseContinuityReference>,
    pub safety_case_reference: Option<SafetyCaseReference>,
    pub evidence_level_reference: Option<EvidenceLevelReference>,
    pub audit_reference: Option<AuditReference>,
    pub reason_fact: Option<ReasonFactReference>,
    pub fact_idempotency_key: Option<C2BoundedPromiseReliabilityFactIdempotencyKey>,
    pub evidence_posture: Option<EvidencePosture>,
    pub reviewability_posture: Option<ReviewabilityPosture>,
    pub retention_posture: Option<RetentionPosture>,
    pub authority_posture: SocialTrustAuthorityPosture,
    pub boundary_posture: C2BoundedPromiseReliabilityBoundaryPosture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilitySourceFactCandidate {
    Accepted(C2BoundedPromiseReliabilitySourceFact),
    Rejected(RejectedC2BoundedPromiseReliabilitySourceFact),
    Unknown,
}

impl C2BoundedPromiseReliabilitySourceFactCandidate {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted(source) => source.as_str(),
            Self::Rejected(source) => source.as_str(),
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilitySourceFact {
    CompletedAsAgreed,
    CompletedAfterGovernedReview,
    ValidExcusedExit,
    SourceFactCorrected,
    ReviewRequiredBoundaryIntersection,
    SourceScopeLimitedAfterReview,
    FreezeOrNarrowingReversedAfterReview,
}

impl C2BoundedPromiseReliabilitySourceFact {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompletedAsAgreed => "promise_reliability_outcome.completed_as_agreed",
            Self::CompletedAfterGovernedReview => {
                "promise_reliability_outcome.completed_after_governed_review"
            }
            Self::ValidExcusedExit => "promise_reliability_outcome.valid_excused_exit",
            Self::SourceFactCorrected => "promise_reliability_outcome.source_fact_corrected",
            Self::ReviewRequiredBoundaryIntersection => {
                "promise_reliability_outcome.review_required_boundary_intersection"
            }
            Self::SourceScopeLimitedAfterReview => {
                "promise_reliability_outcome.source_scope_limited_after_review"
            }
            Self::FreezeOrNarrowingReversedAfterReview => {
                "promise_reliability_outcome.freeze_or_narrowing_reversed_after_review"
            }
        }
    }

    pub const fn expected_mutation(self) -> C2BoundedPromiseReliabilityMutationFact {
        match self {
            Self::CompletedAsAgreed | Self::CompletedAfterGovernedReview => {
                C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive
            }
            Self::ValidExcusedExit => {
                C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit
            }
            Self::SourceFactCorrected => {
                C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityCorrection
            }
            Self::ReviewRequiredBoundaryIntersection => {
                C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze
            }
            Self::SourceScopeLimitedAfterReview => {
                C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityNarrowing
            }
            Self::FreezeOrNarrowingReversedAfterReview => {
                C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityRecovery
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C1FirstPositiveSourceScope {
    FulfilledCommitmentPromiseFollowThrough,
    AccountableCompletionBehavior,
}

impl C1FirstPositiveSourceScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FulfilledCommitmentPromiseFollowThrough => {
                "fulfilled_commitments_promise_follow_through"
            }
            Self::AccountableCompletionBehavior => "accountable_completion_behavior",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C1FirstPositiveSourceScopeDecision {
    Accept {
        source_fact: C2BoundedPromiseReliabilitySourceFact,
        source_scope: C1FirstPositiveSourceScope,
    },
    Reject(C1FirstPositiveSourceScopeRejection),
}

impl C1FirstPositiveSourceScopeDecision {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Accept { .. } => "accept",
            Self::Reject(_) => "rejected",
        }
    }

    pub const fn rejection_reason_code(&self) -> Option<&'static str> {
        match self {
            Self::Accept { .. } => None,
            Self::Reject(rejection) => Some(rejection.as_str()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C1FirstPositiveSourceScopeRejection {
    RejectedSourceFact {
        source: RejectedC2BoundedPromiseReliabilitySourceFact,
    },
    UnknownSourceFact,
    NotFirstPositiveSource {
        source: C2BoundedPromiseReliabilitySourceFact,
    },
}

impl C1FirstPositiveSourceScopeRejection {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::RejectedSourceFact { .. } => "rejected_source_fact",
            Self::UnknownSourceFact => "unknown_source_fact",
            Self::NotFirstPositiveSource { .. } => "not_first_positive_source",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityMutationFactCandidate {
    Accepted(C2BoundedPromiseReliabilityMutationFact),
    Unknown,
}

impl C2BoundedPromiseReliabilityMutationFactCandidate {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted(mutation) => mutation.as_str(),
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityMutationFact {
    BoundedPromiseReliabilityPositive,
    NoEffectValidExcusedExit,
    BoundedPromiseReliabilityCorrection,
    BoundedPromiseReliabilityFreeze,
    BoundedPromiseReliabilityNarrowing,
    BoundedPromiseReliabilityRecovery,
}

impl C2BoundedPromiseReliabilityMutationFact {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BoundedPromiseReliabilityPositive => {
                "social_trust_mutation.bounded_promise_reliability_positive"
            }
            Self::NoEffectValidExcusedExit => "social_trust_mutation.no_effect_valid_excused_exit",
            Self::BoundedPromiseReliabilityCorrection => {
                "social_trust_mutation.bounded_promise_reliability_correction"
            }
            Self::BoundedPromiseReliabilityFreeze => {
                "social_trust_mutation.bounded_promise_reliability_freeze"
            }
            Self::BoundedPromiseReliabilityNarrowing => {
                "social_trust_mutation.bounded_promise_reliability_narrowing"
            }
            Self::BoundedPromiseReliabilityRecovery => {
                "social_trust_mutation.bounded_promise_reliability_recovery"
            }
        }
    }

    pub const fn direction(self) -> C2BoundedPromiseReliabilityMutationDirection {
        match self {
            Self::BoundedPromiseReliabilityPositive => {
                C2BoundedPromiseReliabilityMutationDirection::Positive
            }
            Self::NoEffectValidExcusedExit => {
                C2BoundedPromiseReliabilityMutationDirection::NoEffect
            }
            Self::BoundedPromiseReliabilityCorrection => {
                C2BoundedPromiseReliabilityMutationDirection::Correction
            }
            Self::BoundedPromiseReliabilityFreeze => {
                C2BoundedPromiseReliabilityMutationDirection::Freeze
            }
            Self::BoundedPromiseReliabilityNarrowing => {
                C2BoundedPromiseReliabilityMutationDirection::Narrowing
            }
            Self::BoundedPromiseReliabilityRecovery => {
                C2BoundedPromiseReliabilityMutationDirection::Recovery
            }
        }
    }

    pub const fn magnitude(self) -> C2BoundedPromiseReliabilityMutationMagnitude {
        match self {
            Self::BoundedPromiseReliabilityPositive => {
                C2BoundedPromiseReliabilityMutationMagnitude::Categorical
            }
            Self::NoEffectValidExcusedExit => {
                C2BoundedPromiseReliabilityMutationMagnitude::NoEffect
            }
            Self::BoundedPromiseReliabilityCorrection => {
                C2BoundedPromiseReliabilityMutationMagnitude::ForwardCorrection
            }
            Self::BoundedPromiseReliabilityFreeze => {
                C2BoundedPromiseReliabilityMutationMagnitude::TemporarySuppression
            }
            Self::BoundedPromiseReliabilityNarrowing => {
                C2BoundedPromiseReliabilityMutationMagnitude::ScopeLimitedRestriction
            }
            Self::BoundedPromiseReliabilityRecovery => {
                C2BoundedPromiseReliabilityMutationMagnitude::EligibilityRestoration
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityMutationDirection {
    Positive,
    NoEffect,
    Correction,
    Freeze,
    Narrowing,
    Recovery,
}

impl C2BoundedPromiseReliabilityMutationDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::NoEffect => "no_effect",
            Self::Correction => "correction",
            Self::Freeze => "freeze",
            Self::Narrowing => "narrowing",
            Self::Recovery => "recovery",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityMutationMagnitude {
    Categorical,
    NoEffect,
    ForwardCorrection,
    TemporarySuppression,
    ScopeLimitedRestriction,
    EligibilityRestoration,
}

impl C2BoundedPromiseReliabilityMutationMagnitude {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Categorical => "categorical",
            Self::NoEffect => "no_effect",
            Self::ForwardCorrection => "forward_correction",
            Self::TemporarySuppression => "temporary_suppression",
            Self::ScopeLimitedRestriction => "scope_limited_restriction",
            Self::EligibilityRestoration => "eligibility_restoration",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityBoundaryPosture {
    Clear,
    Unresolved(C2BoundedPromiseReliabilityBoundaryIntersection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityBoundaryIntersection {
    Consent,
    BlockMuteRefusalOrWithdrawal,
    AgeAssurance,
    LegalHold,
    CriticalHarm,
    AccountLifecycle,
    AppealCorrectionOrSafetyReview,
    AntiAbuseSuppression,
    CollusionScamOrCoercion,
    SensitiveExposure,
}

impl C2BoundedPromiseReliabilityBoundaryIntersection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Consent => "consent",
            Self::BlockMuteRefusalOrWithdrawal => "block_mute_refusal_or_withdrawal",
            Self::AgeAssurance => "age_assurance",
            Self::LegalHold => "legal_hold",
            Self::CriticalHarm => "critical_harm",
            Self::AccountLifecycle => "account_lifecycle",
            Self::AppealCorrectionOrSafetyReview => "appeal_correction_or_safety_review",
            Self::AntiAbuseSuppression => "anti_abuse_suppression",
            Self::CollusionScamOrCoercion => "collusion_scam_or_coercion",
            Self::SensitiveExposure => "sensitive_exposure",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectedC2BoundedPromiseReliabilitySourceFact {
    PromiseCreation,
    PromiseAcceptance,
    PromiseTerms,
    PromiseEscrowCreation,
    EscrowAmount,
    EscrowRelease,
    Forfeiture,
    PaymentAmount,
    PaymentFrequency,
    SupportAmount,
    SupportStatus,
    TokenHoldings,
    MeetingAttendanceClaimByOneParty,
    RawVenuePresence,
    RawGps,
    StaticQrScan,
    NfcTapAlone,
    BleObservationAlone,
    BleMacAddress,
    DeviceAttestationAlone,
    MissingDeviceAttestationAlone,
    ProximityProofAlone,
    ProofEligibilityAlone,
    ProofCallbackAlone,
    VendorCallbackAlone,
    ProviderDashboardState,
    ProjectionReadiness,
    ReflectionPraise,
    ApologyText,
    SubjectiveGratitude,
    SinglePartyNarrative,
    ReportCount,
    MassReportCount,
    OperatorNote,
    StewardEndorsementByItself,
    SupportTicket,
    IssueComment,
    Popularity,
    FollowerCount,
    ReplySpeed,
    DwellTime,
    MessageVolume,
    AccountTenure,
    RomanticDesirability,
    EngagementTelemetry,
    RelationshipDepth,
    RoomStateByItself,
    RoomProjection,
    DiscoveryRanking,
    RecommendationState,
    ObservabilityState,
    ModelOutput,
    FrontendState,
    ClientState,
    ControlledExceptionalAccountActivity,
    AgeAssurancePosture,
    VerifiedAdultPosture,
    LegalHoldExistence,
    AntiAbuseContinuityMarkerExistence,
    AccountLifecycleStateByItself,
    DeletionClosureTombstoneAnonymizationKeyShreddingOrReEntry,
    ImplementationConvenience,
}

impl RejectedC2BoundedPromiseReliabilitySourceFact {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PromiseCreation => "promise_creation",
            Self::PromiseAcceptance => "promise_acceptance",
            Self::PromiseTerms => "promise_terms",
            Self::PromiseEscrowCreation => "promise_escrow_creation",
            Self::EscrowAmount => "escrow_amount",
            Self::EscrowRelease => "escrow_release",
            Self::Forfeiture => "forfeiture",
            Self::PaymentAmount => "payment_amount",
            Self::PaymentFrequency => "payment_frequency",
            Self::SupportAmount => "support_amount",
            Self::SupportStatus => "support_status",
            Self::TokenHoldings => "token_holdings",
            Self::MeetingAttendanceClaimByOneParty => "meeting_attendance_claim_by_one_party",
            Self::RawVenuePresence => "raw_venue_presence",
            Self::RawGps => "raw_gps",
            Self::StaticQrScan => "static_qr_scan",
            Self::NfcTapAlone => "nfc_tap_alone",
            Self::BleObservationAlone => "ble_observation_alone",
            Self::BleMacAddress => "ble_mac_address",
            Self::DeviceAttestationAlone => "device_attestation_alone",
            Self::MissingDeviceAttestationAlone => "missing_device_attestation_alone",
            Self::ProximityProofAlone => "proximity_proof_alone",
            Self::ProofEligibilityAlone => "proof_eligibility_alone",
            Self::ProofCallbackAlone => "proof_callback_alone",
            Self::VendorCallbackAlone => "vendor_callback_alone",
            Self::ProviderDashboardState => "provider_dashboard_state",
            Self::ProjectionReadiness => "projection_readiness",
            Self::ReflectionPraise => "reflection_praise",
            Self::ApologyText => "apology_text",
            Self::SubjectiveGratitude => "subjective_gratitude",
            Self::SinglePartyNarrative => "single_party_narrative",
            Self::ReportCount => "report_count",
            Self::MassReportCount => "mass_report_count",
            Self::OperatorNote => "operator_note",
            Self::StewardEndorsementByItself => "steward_endorsement_by_itself",
            Self::SupportTicket => "support_ticket",
            Self::IssueComment => "issue_comment",
            Self::Popularity => "popularity",
            Self::FollowerCount => "follower_count",
            Self::ReplySpeed => "reply_speed",
            Self::DwellTime => "dwell_time",
            Self::MessageVolume => "message_volume",
            Self::AccountTenure => "account_tenure",
            Self::RomanticDesirability => "romantic_desirability",
            Self::EngagementTelemetry => "engagement_telemetry",
            Self::RelationshipDepth => "relationship_depth",
            Self::RoomStateByItself => "room_state_by_itself",
            Self::RoomProjection => "room_projection",
            Self::DiscoveryRanking => "discovery_ranking",
            Self::RecommendationState => "recommendation_state",
            Self::ObservabilityState => "observability_state",
            Self::ModelOutput => "model_output",
            Self::FrontendState => "frontend_state",
            Self::ClientState => "client_state",
            Self::ControlledExceptionalAccountActivity => "controlled_exceptional_account_activity",
            Self::AgeAssurancePosture => "age_assurance_posture",
            Self::VerifiedAdultPosture => "verified_adult_posture",
            Self::LegalHoldExistence => "legal_hold_existence",
            Self::AntiAbuseContinuityMarkerExistence => "anti_abuse_continuity_marker_existence",
            Self::AccountLifecycleStateByItself => "account_lifecycle_state_by_itself",
            Self::DeletionClosureTombstoneAnonymizationKeyShreddingOrReEntry => {
                "deletion_closure_tombstone_anonymization_key_shredding_or_re_entry"
            }
            Self::ImplementationConvenience => "implementation_convenience",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityMutationDecision {
    Persist {
        source_fact: C2BoundedPromiseReliabilitySourceFact,
        mutation_fact: C2BoundedPromiseReliabilityMutationFact,
        direction: C2BoundedPromiseReliabilityMutationDirection,
        magnitude: C2BoundedPromiseReliabilityMutationMagnitude,
    },
    Reject(C2BoundedPromiseReliabilityRejection),
}

impl C2BoundedPromiseReliabilityMutationDecision {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Persist { .. } => "persist",
            Self::Reject(_) => "rejected",
        }
    }

    pub const fn rejection_reason_code(&self) -> Option<&'static str> {
        match self {
            Self::Persist { .. } => None,
            Self::Reject(rejection) => Some(rejection.as_str()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C2CategoricalFactConsumptionAttempt {
    pub source_fact: C2BoundedPromiseReliabilitySourceFact,
    pub mutation_fact: C2BoundedPromiseReliabilityMutationFact,
    pub target: C2CategoricalFactConsumptionTarget,
    pub authority_posture: SocialTrustAuthorityPosture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C2CategoricalFactConsumptionTarget {
    InternalWriterFactReference,
    NumericSocialTrustScore,
    SocialTrustScoreDelta,
    SocialTrustWeight,
    SocialTrustRank,
    SocialTrustDisplayLevel,
    SocialTrustPublicLevel,
    PublicSocialTrustDisplay,
    RecoveryCeiling,
    DiscoveryPriority,
    RecommendationBoost,
    ContactUnlock,
    RoomTransition,
    SettlementProgression,
    PromiseRuntimeOutcome,
    ProofRuntimeOutcome,
    RelationshipDepthFact,
    ProjectionRefresh,
    PublicApiResponse,
    MobileUiState,
}

impl C2CategoricalFactConsumptionTarget {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InternalWriterFactReference => "internal_writer_fact_reference",
            Self::NumericSocialTrustScore => "numeric_social_trust_score",
            Self::SocialTrustScoreDelta => "social_trust_score_delta",
            Self::SocialTrustWeight => "social_trust_weight",
            Self::SocialTrustRank => "social_trust_rank",
            Self::SocialTrustDisplayLevel => "social_trust_display_level",
            Self::SocialTrustPublicLevel => "social_trust_public_level",
            Self::PublicSocialTrustDisplay => "public_social_trust_display",
            Self::RecoveryCeiling => "recovery_ceiling",
            Self::DiscoveryPriority => "discovery_priority",
            Self::RecommendationBoost => "recommendation_boost",
            Self::ContactUnlock => "contact_unlock",
            Self::RoomTransition => "room_transition",
            Self::SettlementProgression => "settlement_progression",
            Self::PromiseRuntimeOutcome => "promise_runtime_outcome",
            Self::ProofRuntimeOutcome => "proof_runtime_outcome",
            Self::RelationshipDepthFact => "relationship_depth_fact",
            Self::ProjectionRefresh => "projection_refresh",
            Self::PublicApiResponse => "public_api_response",
            Self::MobileUiState => "mobile_ui_state",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C2CategoricalFactConsumptionDecision {
    AllowInternalWriterFactReference {
        source_fact: C2BoundedPromiseReliabilitySourceFact,
        mutation_fact: C2BoundedPromiseReliabilityMutationFact,
    },
    Reject(C2CategoricalFactConsumptionRejection),
}

impl C2CategoricalFactConsumptionDecision {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::AllowInternalWriterFactReference { .. } => "allow_internal_writer_fact_reference",
            Self::Reject(_) => "rejected",
        }
    }

    pub const fn rejection_reason_code(&self) -> Option<&'static str> {
        match self {
            Self::AllowInternalWriterFactReference { .. } => None,
            Self::Reject(rejection) => Some(rejection.as_str()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C2CategoricalFactConsumptionRejection {
    SourceMutationMismatch {
        source: C2BoundedPromiseReliabilitySourceFact,
        requested: C2BoundedPromiseReliabilityMutationFact,
        expected: C2BoundedPromiseReliabilityMutationFact,
    },
    ProjectionOnlyAuthority,
    BlockedConsumptionTarget {
        target: C2CategoricalFactConsumptionTarget,
    },
}

impl C2CategoricalFactConsumptionRejection {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SourceMutationMismatch { .. } => "source_mutation_mismatch",
            Self::ProjectionOnlyAuthority => "projection_only_authority",
            Self::BlockedConsumptionTarget { .. } => "blocked_consumption_target",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityRejection {
    RejectedSourceFact {
        source: RejectedC2BoundedPromiseReliabilitySourceFact,
    },
    UnknownSourceFact,
    UnknownMutationFact,
    SourceMutationMismatch {
        source: C2BoundedPromiseReliabilitySourceFact,
        requested: C2BoundedPromiseReliabilityMutationFact,
        expected: C2BoundedPromiseReliabilityMutationFact,
    },
    ProjectionOnlyAuthority,
    MissingReviewRequiredBoundaryIntersection,
    BoundaryUnresolved {
        boundary: C2BoundedPromiseReliabilityBoundaryIntersection,
    },
    MissingWriterSourceReference,
    MissingPromiseReference,
    MissingPromiseTermsReference,
    MissingConsentAtFormationReference,
    MissingConsentAtResolutionReference,
    MissingBlockWithdrawalStateReference,
    MissingAgeAssuranceStateReference,
    MissingLegalHoldIntersectionReference,
    MissingCriticalHarmCaseReference,
    MissingAccountLifecycleReference,
    MissingAntiAbuseContinuityReference,
    MissingSafetyCaseReference,
    MissingEvidenceLevelReference,
    MissingAuditReference,
    MissingReasonFact,
    MissingFactIdempotencyKey,
    MissingEvidencePosture,
    MissingReviewabilityPosture,
    MissingRetentionPosture,
}

impl C2BoundedPromiseReliabilityRejection {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::RejectedSourceFact { .. } => "rejected_source_fact",
            Self::UnknownSourceFact => "unknown_source_fact",
            Self::UnknownMutationFact => "unknown_mutation_fact",
            Self::SourceMutationMismatch { .. } => "source_mutation_mismatch",
            Self::ProjectionOnlyAuthority => "projection_only_authority",
            Self::MissingReviewRequiredBoundaryIntersection => {
                "missing_review_required_boundary_intersection"
            }
            Self::BoundaryUnresolved { .. } => "boundary_unresolved",
            Self::MissingWriterSourceReference => "missing_writer_source_reference",
            Self::MissingPromiseReference => "missing_promise_reference",
            Self::MissingPromiseTermsReference => "missing_promise_terms_reference",
            Self::MissingConsentAtFormationReference => "missing_consent_at_formation_reference",
            Self::MissingConsentAtResolutionReference => "missing_consent_at_resolution_reference",
            Self::MissingBlockWithdrawalStateReference => {
                "missing_block_withdrawal_state_reference"
            }
            Self::MissingAgeAssuranceStateReference => "missing_age_assurance_state_reference",
            Self::MissingLegalHoldIntersectionReference => {
                "missing_legal_hold_intersection_reference"
            }
            Self::MissingCriticalHarmCaseReference => "missing_critical_harm_case_reference",
            Self::MissingAccountLifecycleReference => "missing_account_lifecycle_reference",
            Self::MissingAntiAbuseContinuityReference => "missing_anti_abuse_continuity_reference",
            Self::MissingSafetyCaseReference => "missing_safety_case_reference",
            Self::MissingEvidenceLevelReference => "missing_evidence_level_reference",
            Self::MissingAuditReference => "missing_audit_reference",
            Self::MissingReasonFact => "missing_reason_fact",
            Self::MissingFactIdempotencyKey => "missing_fact_idempotency_key",
            Self::MissingEvidencePosture => "missing_evidence_posture",
            Self::MissingReviewabilityPosture => "missing_reviewability_posture",
            Self::MissingRetentionPosture => "missing_retention_posture",
        }
    }
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

#[must_use]
pub const fn decide_c1_first_positive_source_scope(
    candidate: C2BoundedPromiseReliabilitySourceFactCandidate,
) -> C1FirstPositiveSourceScopeDecision {
    match candidate {
        C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        ) => C1FirstPositiveSourceScopeDecision::Accept {
            source_fact: C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            source_scope: C1FirstPositiveSourceScope::FulfilledCommitmentPromiseFollowThrough,
        },
        C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(
            C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview,
        ) => C1FirstPositiveSourceScopeDecision::Accept {
            source_fact: C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview,
            source_scope: C1FirstPositiveSourceScope::AccountableCompletionBehavior,
        },
        C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source) => {
            C1FirstPositiveSourceScopeDecision::Reject(
                C1FirstPositiveSourceScopeRejection::NotFirstPositiveSource { source },
            )
        }
        C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(source) => {
            C1FirstPositiveSourceScopeDecision::Reject(
                C1FirstPositiveSourceScopeRejection::RejectedSourceFact { source },
            )
        }
        C2BoundedPromiseReliabilitySourceFactCandidate::Unknown => {
            C1FirstPositiveSourceScopeDecision::Reject(
                C1FirstPositiveSourceScopeRejection::UnknownSourceFact,
            )
        }
    }
}

#[must_use]
pub fn decide_c2_bounded_promise_reliability_mutation(
    proposal: &ProposedC2BoundedPromiseReliabilityMutationFact,
) -> C2BoundedPromiseReliabilityMutationDecision {
    let source_fact = match proposal.source_fact {
        C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source) => source,
        C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(source) => {
            return C2BoundedPromiseReliabilityMutationDecision::Reject(
                C2BoundedPromiseReliabilityRejection::RejectedSourceFact { source },
            );
        }
        C2BoundedPromiseReliabilitySourceFactCandidate::Unknown => {
            return C2BoundedPromiseReliabilityMutationDecision::Reject(
                C2BoundedPromiseReliabilityRejection::UnknownSourceFact,
            );
        }
    };

    let requested_mutation = match proposal.requested_mutation_fact {
        C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(mutation) => mutation,
        C2BoundedPromiseReliabilityMutationFactCandidate::Unknown => {
            return C2BoundedPromiseReliabilityMutationDecision::Reject(
                C2BoundedPromiseReliabilityRejection::UnknownMutationFact,
            );
        }
    };
    let expected_mutation = source_fact.expected_mutation();
    if requested_mutation != expected_mutation {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::SourceMutationMismatch {
                source: source_fact,
                requested: requested_mutation,
                expected: expected_mutation,
            },
        );
    }

    if proposal.authority_posture == SocialTrustAuthorityPosture::ProjectionOnly {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::ProjectionOnlyAuthority,
        );
    }

    match proposal.boundary_posture {
        C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(boundary) => {
            if source_fact
                != C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection
            {
                return C2BoundedPromiseReliabilityMutationDecision::Reject(
                    C2BoundedPromiseReliabilityRejection::BoundaryUnresolved { boundary },
                );
            }
        }
        C2BoundedPromiseReliabilityBoundaryPosture::Clear
            if source_fact
                == C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection =>
        {
            return C2BoundedPromiseReliabilityMutationDecision::Reject(
                C2BoundedPromiseReliabilityRejection::MissingReviewRequiredBoundaryIntersection,
            );
        }
        C2BoundedPromiseReliabilityBoundaryPosture::Clear => {}
    }

    if !proposal
        .writer_source_reference
        .as_ref()
        .is_some_and(WriterSourceReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingWriterSourceReference,
        );
    }
    if !proposal
        .promise_reference
        .as_ref()
        .is_some_and(PromiseReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingPromiseReference,
        );
    }
    if !proposal
        .promise_terms_reference
        .as_ref()
        .is_some_and(PromiseTermsReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingPromiseTermsReference,
        );
    }
    if !proposal
        .consent_at_formation_reference
        .as_ref()
        .is_some_and(ConsentStateReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingConsentAtFormationReference,
        );
    }
    if !proposal
        .consent_at_resolution_reference
        .as_ref()
        .is_some_and(ConsentStateReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingConsentAtResolutionReference,
        );
    }
    if !proposal
        .block_withdrawal_state_reference
        .as_ref()
        .is_some_and(BlockWithdrawalStateReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingBlockWithdrawalStateReference,
        );
    }
    if !proposal
        .age_assurance_state_reference
        .as_ref()
        .is_some_and(AgeAssuranceStateReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingAgeAssuranceStateReference,
        );
    }
    if !proposal
        .legal_hold_intersection_reference
        .as_ref()
        .is_some_and(LegalHoldIntersectionReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingLegalHoldIntersectionReference,
        );
    }
    if !proposal
        .critical_harm_case_reference
        .as_ref()
        .is_some_and(CriticalHarmCaseReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingCriticalHarmCaseReference,
        );
    }
    if !proposal
        .account_lifecycle_reference
        .as_ref()
        .is_some_and(AccountLifecycleReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingAccountLifecycleReference,
        );
    }
    if !proposal
        .anti_abuse_continuity_reference
        .as_ref()
        .is_some_and(AntiAbuseContinuityReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingAntiAbuseContinuityReference,
        );
    }
    if !proposal
        .safety_case_reference
        .as_ref()
        .is_some_and(SafetyCaseReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingSafetyCaseReference,
        );
    }
    if !proposal
        .evidence_level_reference
        .as_ref()
        .is_some_and(EvidenceLevelReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingEvidenceLevelReference,
        );
    }
    if !proposal
        .audit_reference
        .as_ref()
        .is_some_and(AuditReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingAuditReference,
        );
    }
    if !proposal
        .reason_fact
        .as_ref()
        .is_some_and(ReasonFactReference::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingReasonFact,
        );
    }
    if !proposal
        .fact_idempotency_key
        .as_ref()
        .is_some_and(C2BoundedPromiseReliabilityFactIdempotencyKey::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingFactIdempotencyKey,
        );
    }
    if proposal.evidence_posture.is_none() {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingEvidencePosture,
        );
    }
    if proposal.reviewability_posture.is_none() {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingReviewabilityPosture,
        );
    }
    if !proposal
        .retention_posture
        .as_ref()
        .is_some_and(RetentionPosture::is_present)
    {
        return C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingRetentionPosture,
        );
    }

    C2BoundedPromiseReliabilityMutationDecision::Persist {
        source_fact,
        mutation_fact: requested_mutation,
        direction: requested_mutation.direction(),
        magnitude: requested_mutation.magnitude(),
    }
}

#[must_use]
pub fn decide_c2_categorical_fact_consumption(
    attempt: &C2CategoricalFactConsumptionAttempt,
) -> C2CategoricalFactConsumptionDecision {
    let expected_mutation = attempt.source_fact.expected_mutation();
    if attempt.mutation_fact != expected_mutation {
        return C2CategoricalFactConsumptionDecision::Reject(
            C2CategoricalFactConsumptionRejection::SourceMutationMismatch {
                source: attempt.source_fact,
                requested: attempt.mutation_fact,
                expected: expected_mutation,
            },
        );
    }

    if attempt.authority_posture == SocialTrustAuthorityPosture::ProjectionOnly {
        return C2CategoricalFactConsumptionDecision::Reject(
            C2CategoricalFactConsumptionRejection::ProjectionOnlyAuthority,
        );
    }

    if attempt.target != C2CategoricalFactConsumptionTarget::InternalWriterFactReference {
        return C2CategoricalFactConsumptionDecision::Reject(
            C2CategoricalFactConsumptionRejection::BlockedConsumptionTarget {
                target: attempt.target,
            },
        );
    }

    C2CategoricalFactConsumptionDecision::AllowInternalWriterFactReference {
        source_fact: attempt.source_fact,
        mutation_fact: attempt.mutation_fact,
    }
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
string_ref!(C2BoundedPromiseReliabilityFactIdempotencyKey);
string_ref!(PromiseReference);
string_ref!(PromiseTermsReference);
string_ref!(ConsentStateReference);
string_ref!(BlockWithdrawalStateReference);
string_ref!(AgeAssuranceStateReference);
string_ref!(LegalHoldIntersectionReference);
string_ref!(CriticalHarmCaseReference);
string_ref!(AccountLifecycleReference);
string_ref!(AntiAbuseContinuityReference);
string_ref!(SafetyCaseReference);
string_ref!(EvidenceLevelReference);
string_ref!(AuditReference);

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
