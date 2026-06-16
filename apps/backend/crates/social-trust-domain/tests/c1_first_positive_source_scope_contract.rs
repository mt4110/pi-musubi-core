use musubi_social_trust_domain::{
    C1FirstPositiveSourceScope, C1FirstPositiveSourceScopeDecision,
    C1FirstPositiveSourceScopeRejection, C2BoundedPromiseReliabilitySourceFact,
    C2BoundedPromiseReliabilitySourceFactCandidate, RejectedC2BoundedPromiseReliabilitySourceFact,
    decide_c1_first_positive_source_scope,
};

#[test]
fn exact_handoff_labels_are_the_only_c1_first_positive_sources() {
    for (source_fact, source_scope) in accepted_first_positive_sources() {
        assert_eq!(
            decide_c1_first_positive_source_scope(
                C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source_fact),
            ),
            C1FirstPositiveSourceScopeDecision::Accept {
                source_fact,
                source_scope,
            }
        );
    }
}

#[test]
fn handoff_labels_keep_their_accepted_foundation_strings() {
    assert_eq!(
        C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed.as_str(),
        "promise_reliability_outcome.completed_as_agreed"
    );
    assert_eq!(
        C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview.as_str(),
        "promise_reliability_outcome.completed_after_governed_review"
    );
    assert_eq!(
        C1FirstPositiveSourceScope::FulfilledCommitmentPromiseFollowThrough.as_str(),
        "fulfilled_commitments_promise_follow_through"
    );
    assert_eq!(
        C1FirstPositiveSourceScope::AccountableCompletionBehavior.as_str(),
        "accountable_completion_behavior"
    );
}

#[test]
fn nearby_accepted_c2_sources_are_not_c1_first_positive_sources() {
    for source in non_positive_accepted_sources() {
        assert_eq!(
            decide_c1_first_positive_source_scope(
                C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source),
            ),
            C1FirstPositiveSourceScopeDecision::Reject(
                C1FirstPositiveSourceScopeRejection::NotFirstPositiveSource { source }
            ),
            "{} must not enter the C1 first positive source scope",
            source.as_str(),
        );
    }
}

#[test]
fn rejected_promise_reliability_sources_fail_closed_for_c1_first_positive_scope() {
    for source in rejected_source_facts() {
        assert_eq!(
            decide_c1_first_positive_source_scope(
                C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(source),
            ),
            C1FirstPositiveSourceScopeDecision::Reject(
                C1FirstPositiveSourceScopeRejection::RejectedSourceFact { source }
            ),
            "{} must remain non-authority for the C1 first positive source scope",
            source.as_str(),
        );
    }
}

#[test]
fn hard_exclusion_shortcuts_never_enter_c1_first_positive_scope() {
    for source in hard_exclusion_shortcuts() {
        assert_eq!(
            decide_c1_first_positive_source_scope(
                C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(source),
            ),
            C1FirstPositiveSourceScopeDecision::Reject(
                C1FirstPositiveSourceScopeRejection::RejectedSourceFact { source }
            ),
            "{} must not become a C1 first positive source",
            source.as_str(),
        );
    }
}

#[test]
fn unknown_source_fact_fails_closed_for_c1_first_positive_scope() {
    assert_eq!(
        decide_c1_first_positive_source_scope(
            C2BoundedPromiseReliabilitySourceFactCandidate::Unknown,
        ),
        C1FirstPositiveSourceScopeDecision::Reject(
            C1FirstPositiveSourceScopeRejection::UnknownSourceFact
        )
    );
}

fn accepted_first_positive_sources() -> Vec<(
    C2BoundedPromiseReliabilitySourceFact,
    C1FirstPositiveSourceScope,
)> {
    vec![
        (
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            C1FirstPositiveSourceScope::FulfilledCommitmentPromiseFollowThrough,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview,
            C1FirstPositiveSourceScope::AccountableCompletionBehavior,
        ),
    ]
}

fn non_positive_accepted_sources() -> Vec<C2BoundedPromiseReliabilitySourceFact> {
    vec![
        C2BoundedPromiseReliabilitySourceFact::ValidExcusedExit,
        C2BoundedPromiseReliabilitySourceFact::SourceFactCorrected,
        C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
        C2BoundedPromiseReliabilitySourceFact::SourceScopeLimitedAfterReview,
        C2BoundedPromiseReliabilitySourceFact::FreezeOrNarrowingReversedAfterReview,
    ]
}

fn rejected_source_facts() -> Vec<RejectedC2BoundedPromiseReliabilitySourceFact> {
    vec![
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseCreation,
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseAcceptance,
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseTerms,
        RejectedC2BoundedPromiseReliabilitySourceFact::PromiseEscrowCreation,
        RejectedC2BoundedPromiseReliabilitySourceFact::EscrowAmount,
        RejectedC2BoundedPromiseReliabilitySourceFact::EscrowRelease,
        RejectedC2BoundedPromiseReliabilitySourceFact::Forfeiture,
        RejectedC2BoundedPromiseReliabilitySourceFact::PaymentAmount,
        RejectedC2BoundedPromiseReliabilitySourceFact::PaymentFrequency,
        RejectedC2BoundedPromiseReliabilitySourceFact::SupportAmount,
        RejectedC2BoundedPromiseReliabilitySourceFact::SupportStatus,
        RejectedC2BoundedPromiseReliabilitySourceFact::TokenHoldings,
        RejectedC2BoundedPromiseReliabilitySourceFact::MeetingAttendanceClaimByOneParty,
        RejectedC2BoundedPromiseReliabilitySourceFact::RawVenuePresence,
        RejectedC2BoundedPromiseReliabilitySourceFact::RawGps,
        RejectedC2BoundedPromiseReliabilitySourceFact::StaticQrScan,
        RejectedC2BoundedPromiseReliabilitySourceFact::NfcTapAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::BleObservationAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::BleMacAddress,
        RejectedC2BoundedPromiseReliabilitySourceFact::DeviceAttestationAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::MissingDeviceAttestationAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProximityProofAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProofEligibilityAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProofCallbackAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::VendorCallbackAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProviderDashboardState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProjectionReadiness,
        RejectedC2BoundedPromiseReliabilitySourceFact::ReflectionPraise,
        RejectedC2BoundedPromiseReliabilitySourceFact::ApologyText,
        RejectedC2BoundedPromiseReliabilitySourceFact::SubjectiveGratitude,
        RejectedC2BoundedPromiseReliabilitySourceFact::SinglePartyNarrative,
        RejectedC2BoundedPromiseReliabilitySourceFact::ReportCount,
        RejectedC2BoundedPromiseReliabilitySourceFact::MassReportCount,
        RejectedC2BoundedPromiseReliabilitySourceFact::OperatorNote,
        RejectedC2BoundedPromiseReliabilitySourceFact::StewardEndorsementByItself,
        RejectedC2BoundedPromiseReliabilitySourceFact::SupportTicket,
        RejectedC2BoundedPromiseReliabilitySourceFact::IssueComment,
        RejectedC2BoundedPromiseReliabilitySourceFact::Popularity,
        RejectedC2BoundedPromiseReliabilitySourceFact::FollowerCount,
        RejectedC2BoundedPromiseReliabilitySourceFact::ReplySpeed,
        RejectedC2BoundedPromiseReliabilitySourceFact::DwellTime,
        RejectedC2BoundedPromiseReliabilitySourceFact::MessageVolume,
        RejectedC2BoundedPromiseReliabilitySourceFact::AccountTenure,
        RejectedC2BoundedPromiseReliabilitySourceFact::RomanticDesirability,
        RejectedC2BoundedPromiseReliabilitySourceFact::EngagementTelemetry,
        RejectedC2BoundedPromiseReliabilitySourceFact::RelationshipDepth,
        RejectedC2BoundedPromiseReliabilitySourceFact::RoomStateByItself,
        RejectedC2BoundedPromiseReliabilitySourceFact::RoomProjection,
        RejectedC2BoundedPromiseReliabilitySourceFact::DiscoveryRanking,
        RejectedC2BoundedPromiseReliabilitySourceFact::RecommendationState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ObservabilityState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ModelOutput,
        RejectedC2BoundedPromiseReliabilitySourceFact::FrontendState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ClientState,
        RejectedC2BoundedPromiseReliabilitySourceFact::ControlledExceptionalAccountActivity,
        RejectedC2BoundedPromiseReliabilitySourceFact::AgeAssurancePosture,
        RejectedC2BoundedPromiseReliabilitySourceFact::VerifiedAdultPosture,
        RejectedC2BoundedPromiseReliabilitySourceFact::LegalHoldExistence,
        RejectedC2BoundedPromiseReliabilitySourceFact::AntiAbuseContinuityMarkerExistence,
        RejectedC2BoundedPromiseReliabilitySourceFact::AccountLifecycleStateByItself,
        RejectedC2BoundedPromiseReliabilitySourceFact::DeletionClosureTombstoneAnonymizationKeyShreddingOrReEntry,
        RejectedC2BoundedPromiseReliabilitySourceFact::ImplementationConvenience,
    ]
}

fn hard_exclusion_shortcuts() -> Vec<RejectedC2BoundedPromiseReliabilitySourceFact> {
    vec![
        RejectedC2BoundedPromiseReliabilitySourceFact::EscrowRelease,
        RejectedC2BoundedPromiseReliabilitySourceFact::Forfeiture,
        RejectedC2BoundedPromiseReliabilitySourceFact::PaymentAmount,
        RejectedC2BoundedPromiseReliabilitySourceFact::SupportStatus,
        RejectedC2BoundedPromiseReliabilitySourceFact::TokenHoldings,
        RejectedC2BoundedPromiseReliabilitySourceFact::RawVenuePresence,
        RejectedC2BoundedPromiseReliabilitySourceFact::RawGps,
        RejectedC2BoundedPromiseReliabilitySourceFact::StaticQrScan,
        RejectedC2BoundedPromiseReliabilitySourceFact::DeviceAttestationAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProximityProofAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProofCallbackAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::VendorCallbackAlone,
        RejectedC2BoundedPromiseReliabilitySourceFact::ProjectionReadiness,
        RejectedC2BoundedPromiseReliabilitySourceFact::OperatorNote,
        RejectedC2BoundedPromiseReliabilitySourceFact::SinglePartyNarrative,
        RejectedC2BoundedPromiseReliabilitySourceFact::Popularity,
        RejectedC2BoundedPromiseReliabilitySourceFact::MessageVolume,
        RejectedC2BoundedPromiseReliabilitySourceFact::RomanticDesirability,
        RejectedC2BoundedPromiseReliabilitySourceFact::RelationshipDepth,
        RejectedC2BoundedPromiseReliabilitySourceFact::ControlledExceptionalAccountActivity,
        RejectedC2BoundedPromiseReliabilitySourceFact::AgeAssurancePosture,
        RejectedC2BoundedPromiseReliabilitySourceFact::LegalHoldExistence,
        RejectedC2BoundedPromiseReliabilitySourceFact::AntiAbuseContinuityMarkerExistence,
        RejectedC2BoundedPromiseReliabilitySourceFact::ImplementationConvenience,
    ]
}
