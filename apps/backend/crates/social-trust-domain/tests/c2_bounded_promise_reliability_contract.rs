use musubi_social_trust_domain::{
    AccountLifecycleReference, AgeAssuranceStateReference, AntiAbuseContinuityReference,
    AuditReference, BlockWithdrawalStateReference, C2BoundedPromiseReliabilityBoundaryIntersection,
    C2BoundedPromiseReliabilityBoundaryPosture, C2BoundedPromiseReliabilityFactIdempotencyKey,
    C2BoundedPromiseReliabilityMutationDecision, C2BoundedPromiseReliabilityMutationFact,
    C2BoundedPromiseReliabilityMutationFactCandidate, C2BoundedPromiseReliabilityMutationMagnitude,
    C2BoundedPromiseReliabilityRejection, C2BoundedPromiseReliabilitySourceFact,
    C2BoundedPromiseReliabilitySourceFactCandidate, ConsentStateReference,
    CriticalHarmCaseReference, EvidenceLevelReference, EvidencePosture,
    LegalHoldIntersectionReference, PromiseReference, PromiseTermsReference,
    ProposedC2BoundedPromiseReliabilityMutationFact, ReasonFactReference,
    RejectedC2BoundedPromiseReliabilitySourceFact, RetentionClassReference, RetentionPosture,
    ReviewabilityPosture, SafetyCaseReference, SocialTrustAuthorityPosture, WriterSourceReference,
    decide_c2_bounded_promise_reliability_mutation,
};

#[test]
fn accepted_source_facts_map_only_to_exact_categorical_mutation_facts() {
    for (source, mutation, magnitude) in accepted_mappings() {
        let mut proposal = complete_proposal();
        proposal.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source);
        proposal.requested_mutation_fact =
            C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(mutation);
        if source == C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection {
            proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
                C2BoundedPromiseReliabilityBoundaryIntersection::AppealCorrectionOrSafetyReview,
            );
        }

        let decision = decide_c2_bounded_promise_reliability_mutation(&proposal);

        assert_eq!(
            decision,
            C2BoundedPromiseReliabilityMutationDecision::Persist {
                source_fact: source,
                mutation_fact: mutation,
                direction: mutation.direction(),
                magnitude
            }
        );
    }
}

#[test]
fn accepted_source_fact_with_wrong_mutation_fact_fails_closed() {
    for (source, expected, _) in accepted_mappings() {
        for requested in accepted_mutation_facts() {
            if requested == expected {
                continue;
            }
            let mut proposal = complete_proposal();
            proposal.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(source);
            proposal.requested_mutation_fact =
                C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(requested);

            let decision = decide_c2_bounded_promise_reliability_mutation(&proposal);

            assert_eq!(
                decision,
                C2BoundedPromiseReliabilityMutationDecision::Reject(
                    C2BoundedPromiseReliabilityRejection::SourceMutationMismatch {
                        source,
                        requested,
                        expected
                    }
                )
            );
        }
    }
}

#[test]
fn rejected_source_facts_fail_closed() {
    for source in rejected_source_facts() {
        let mut proposal = complete_proposal();
        proposal.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(source);

        let decision = decide_c2_bounded_promise_reliability_mutation(&proposal);

        assert_eq!(
            decision,
            C2BoundedPromiseReliabilityMutationDecision::Reject(
                C2BoundedPromiseReliabilityRejection::RejectedSourceFact { source }
            )
        );
    }
}

#[test]
fn representable_hard_exclusion_source_families_fail_closed_for_c2_mutation() {
    for (family, sources) in hard_exclusion_source_families() {
        for source in sources {
            let mut proposal = complete_proposal();
            proposal.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Rejected(source);

            assert_eq!(
                decide_c2_bounded_promise_reliability_mutation(&proposal),
                C2BoundedPromiseReliabilityMutationDecision::Reject(
                    C2BoundedPromiseReliabilityRejection::RejectedSourceFact { source }
                ),
                "{family} shortcut {} must fail closed for C2 mutation",
                source.as_str(),
            );
        }
    }
}

#[test]
fn unknown_source_or_mutation_fact_fails_closed() {
    let mut unknown_source = complete_proposal();
    unknown_source.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Unknown;

    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&unknown_source),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::UnknownSourceFact
        )
    );

    let mut unknown_mutation = complete_proposal();
    unknown_mutation.requested_mutation_fact =
        C2BoundedPromiseReliabilityMutationFactCandidate::Unknown;

    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&unknown_mutation),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::UnknownMutationFact
        )
    );
}

#[test]
fn projection_authority_and_unresolved_boundaries_fail_closed() {
    let mut projection_only = complete_proposal();
    projection_only.authority_posture = SocialTrustAuthorityPosture::ProjectionOnly;

    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&projection_only),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::ProjectionOnlyAuthority
        )
    );

    let mut unresolved = complete_proposal();
    unresolved.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
        C2BoundedPromiseReliabilityBoundaryIntersection::Consent,
    );

    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&unresolved),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::BoundaryUnresolved {
                boundary: C2BoundedPromiseReliabilityBoundaryIntersection::Consent
            }
        )
    );
}

#[test]
fn ordinary_positive_completion_paths_fail_closed_on_each_boundary_intersection() {
    for boundary in boundary_intersections() {
        let mut proposal = complete_proposal();
        proposal.boundary_posture =
            C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(boundary);

        assert_eq!(
            decide_c2_bounded_promise_reliability_mutation(&proposal),
            C2BoundedPromiseReliabilityMutationDecision::Reject(
                C2BoundedPromiseReliabilityRejection::BoundaryUnresolved { boundary }
            ),
            "ordinary positive completion path must not bypass {}",
            boundary.as_str(),
        );
    }
}

#[test]
fn review_required_boundary_intersection_can_persist_categorical_freeze() {
    let mut proposal = complete_proposal();
    proposal.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(
        C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
    );
    proposal.requested_mutation_fact = C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
    );
    proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Unresolved(
        C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
    );

    let decision = decide_c2_bounded_promise_reliability_mutation(&proposal);

    assert_eq!(
        decision,
        C2BoundedPromiseReliabilityMutationDecision::Persist {
            source_fact: C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
            mutation_fact: C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
            direction: C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze
                .direction(),
            magnitude: C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze
                .magnitude(),
        }
    );
}

#[test]
fn review_required_boundary_intersection_requires_unresolved_boundary() {
    let mut proposal = complete_proposal();
    proposal.source_fact = C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(
        C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
    );
    proposal.requested_mutation_fact = C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
    );
    proposal.boundary_posture = C2BoundedPromiseReliabilityBoundaryPosture::Clear;

    let decision = decide_c2_bounded_promise_reliability_mutation(&proposal);

    assert_eq!(
        decision,
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingReviewRequiredBoundaryIntersection
        )
    );
}

#[test]
fn missing_required_references_and_postures_fail_closed() {
    let mut missing_idempotency = complete_proposal();
    missing_idempotency.fact_idempotency_key = None;
    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&missing_idempotency),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingFactIdempotencyKey
        )
    );

    let mut missing_block_withdrawal = complete_proposal();
    missing_block_withdrawal.block_withdrawal_state_reference = None;
    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&missing_block_withdrawal),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingBlockWithdrawalStateReference
        )
    );

    let mut missing_age_assurance = complete_proposal();
    missing_age_assurance.age_assurance_state_reference = None;
    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&missing_age_assurance),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingAgeAssuranceStateReference
        )
    );

    let mut missing_legal_hold = complete_proposal();
    missing_legal_hold.legal_hold_intersection_reference = None;
    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&missing_legal_hold),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingLegalHoldIntersectionReference
        )
    );

    let mut missing_critical_harm = complete_proposal();
    missing_critical_harm.critical_harm_case_reference = None;
    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&missing_critical_harm),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingCriticalHarmCaseReference
        )
    );

    let mut missing_retention = complete_proposal();
    missing_retention.retention_posture = None;
    assert_eq!(
        decide_c2_bounded_promise_reliability_mutation(&missing_retention),
        C2BoundedPromiseReliabilityMutationDecision::Reject(
            C2BoundedPromiseReliabilityRejection::MissingRetentionPosture
        )
    );
}

fn complete_proposal() -> ProposedC2BoundedPromiseReliabilityMutationFact {
    ProposedC2BoundedPromiseReliabilityMutationFact {
        source_fact: C2BoundedPromiseReliabilitySourceFactCandidate::Accepted(
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        ),
        requested_mutation_fact: C2BoundedPromiseReliabilityMutationFactCandidate::Accepted(
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        ),
        writer_source_reference: Some(WriterSourceReference::new("writer-source-fact-1")),
        promise_reference: Some(PromiseReference::new("promise-1")),
        promise_terms_reference: Some(PromiseTermsReference::new("promise-terms-1")),
        consent_at_formation_reference: Some(ConsentStateReference::new("consent-at-formation-1")),
        consent_at_resolution_reference: Some(ConsentStateReference::new(
            "consent-at-resolution-1",
        )),
        block_withdrawal_state_reference: Some(BlockWithdrawalStateReference::new(
            "block-withdrawal-clear-1",
        )),
        age_assurance_state_reference: Some(AgeAssuranceStateReference::new(
            "age-assurance-adult-eligible-1",
        )),
        legal_hold_intersection_reference: Some(LegalHoldIntersectionReference::new(
            "legal-hold-clear-1",
        )),
        critical_harm_case_reference: Some(CriticalHarmCaseReference::new("critical-harm-clear-1")),
        account_lifecycle_reference: Some(AccountLifecycleReference::new(
            "account-lifecycle-active-1",
        )),
        anti_abuse_continuity_reference: Some(AntiAbuseContinuityReference::new(
            "anti-abuse-clear-1",
        )),
        safety_case_reference: Some(SafetyCaseReference::new("safety-case-clear-1")),
        evidence_level_reference: Some(EvidenceLevelReference::new("evidence-level-bounded-1")),
        audit_reference: Some(AuditReference::new("audit-1")),
        reason_fact: Some(ReasonFactReference::new("reason-fact-1")),
        fact_idempotency_key: Some(C2BoundedPromiseReliabilityFactIdempotencyKey::new(
            "dedupe-1",
        )),
        evidence_posture: Some(EvidencePosture::Bounded),
        reviewability_posture: Some(ReviewabilityPosture::Reviewable),
        retention_posture: Some(RetentionPosture::Classified(RetentionClassReference::new(
            "R4 Trust / moderation / case",
        ))),
        authority_posture: SocialTrustAuthorityPosture::WriterTruthOnly,
        boundary_posture: C2BoundedPromiseReliabilityBoundaryPosture::Clear,
    }
}

fn accepted_mappings() -> Vec<(
    C2BoundedPromiseReliabilitySourceFact,
    C2BoundedPromiseReliabilityMutationFact,
    C2BoundedPromiseReliabilityMutationMagnitude,
)> {
    vec![
        (
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
            C2BoundedPromiseReliabilityMutationMagnitude::Categorical,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
            C2BoundedPromiseReliabilityMutationMagnitude::Categorical,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::ValidExcusedExit,
            C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit,
            C2BoundedPromiseReliabilityMutationMagnitude::NoEffect,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::SourceFactCorrected,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityCorrection,
            C2BoundedPromiseReliabilityMutationMagnitude::ForwardCorrection,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
            C2BoundedPromiseReliabilityMutationMagnitude::TemporarySuppression,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::SourceScopeLimitedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityNarrowing,
            C2BoundedPromiseReliabilityMutationMagnitude::ScopeLimitedRestriction,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::FreezeOrNarrowingReversedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityRecovery,
            C2BoundedPromiseReliabilityMutationMagnitude::EligibilityRestoration,
        ),
    ]
}

fn accepted_mutation_facts() -> Vec<C2BoundedPromiseReliabilityMutationFact> {
    vec![
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityCorrection,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityNarrowing,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityRecovery,
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

fn hard_exclusion_source_families() -> Vec<(
    &'static str,
    Vec<RejectedC2BoundedPromiseReliabilitySourceFact>,
)> {
    vec![
        (
            "payment_support_and_settlement",
            vec![
                RejectedC2BoundedPromiseReliabilitySourceFact::PromiseEscrowCreation,
                RejectedC2BoundedPromiseReliabilitySourceFact::EscrowAmount,
                RejectedC2BoundedPromiseReliabilitySourceFact::EscrowRelease,
                RejectedC2BoundedPromiseReliabilitySourceFact::Forfeiture,
                RejectedC2BoundedPromiseReliabilitySourceFact::PaymentAmount,
                RejectedC2BoundedPromiseReliabilitySourceFact::PaymentFrequency,
                RejectedC2BoundedPromiseReliabilitySourceFact::SupportAmount,
                RejectedC2BoundedPromiseReliabilitySourceFact::SupportStatus,
                RejectedC2BoundedPromiseReliabilitySourceFact::TokenHoldings,
            ],
        ),
        (
            "proof_provider_and_raw_presence",
            vec![
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
            ],
        ),
        (
            "projection_client_model_and_observability",
            vec![
                RejectedC2BoundedPromiseReliabilitySourceFact::ProjectionReadiness,
                RejectedC2BoundedPromiseReliabilitySourceFact::RoomProjection,
                RejectedC2BoundedPromiseReliabilitySourceFact::ObservabilityState,
                RejectedC2BoundedPromiseReliabilitySourceFact::ModelOutput,
                RejectedC2BoundedPromiseReliabilitySourceFact::FrontendState,
                RejectedC2BoundedPromiseReliabilitySourceFact::ClientState,
            ],
        ),
        (
            "subjective_narrative_and_operator_notes",
            vec![
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
            ],
        ),
        (
            "popularity_engagement_and_relationship_depth",
            vec![
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
                RejectedC2BoundedPromiseReliabilitySourceFact::DiscoveryRanking,
                RejectedC2BoundedPromiseReliabilitySourceFact::RecommendationState,
            ],
        ),
        (
            "identity_lifecycle_and_convenience",
            vec![
                RejectedC2BoundedPromiseReliabilitySourceFact::ControlledExceptionalAccountActivity,
                RejectedC2BoundedPromiseReliabilitySourceFact::AgeAssurancePosture,
                RejectedC2BoundedPromiseReliabilitySourceFact::VerifiedAdultPosture,
                RejectedC2BoundedPromiseReliabilitySourceFact::LegalHoldExistence,
                RejectedC2BoundedPromiseReliabilitySourceFact::AntiAbuseContinuityMarkerExistence,
                RejectedC2BoundedPromiseReliabilitySourceFact::AccountLifecycleStateByItself,
                RejectedC2BoundedPromiseReliabilitySourceFact::DeletionClosureTombstoneAnonymizationKeyShreddingOrReEntry,
                RejectedC2BoundedPromiseReliabilitySourceFact::ImplementationConvenience,
            ],
        ),
    ]
}

fn boundary_intersections() -> Vec<C2BoundedPromiseReliabilityBoundaryIntersection> {
    vec![
        C2BoundedPromiseReliabilityBoundaryIntersection::Consent,
        C2BoundedPromiseReliabilityBoundaryIntersection::BlockMuteRefusalOrWithdrawal,
        C2BoundedPromiseReliabilityBoundaryIntersection::AgeAssurance,
        C2BoundedPromiseReliabilityBoundaryIntersection::LegalHold,
        C2BoundedPromiseReliabilityBoundaryIntersection::CriticalHarm,
        C2BoundedPromiseReliabilityBoundaryIntersection::AccountLifecycle,
        C2BoundedPromiseReliabilityBoundaryIntersection::AppealCorrectionOrSafetyReview,
        C2BoundedPromiseReliabilityBoundaryIntersection::AntiAbuseSuppression,
        C2BoundedPromiseReliabilityBoundaryIntersection::CollusionScamOrCoercion,
        C2BoundedPromiseReliabilityBoundaryIntersection::SensitiveExposure,
    ]
}
