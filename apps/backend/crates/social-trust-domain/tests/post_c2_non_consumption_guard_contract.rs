use musubi_social_trust_domain::{
    C2BoundedPromiseReliabilityMutationFact, C2BoundedPromiseReliabilitySourceFact,
    C2CategoricalFactConsumptionAttempt, C2CategoricalFactConsumptionDecision,
    C2CategoricalFactConsumptionRejection, C2CategoricalFactConsumptionTarget,
    SocialTrustAuthorityPosture, decide_c2_categorical_fact_consumption,
};

#[test]
fn exact_c2_pairs_may_remain_internal_writer_fact_references_only() {
    for (source_fact, mutation_fact) in accepted_mappings() {
        let attempt = attempt(
            source_fact,
            mutation_fact,
            C2CategoricalFactConsumptionTarget::InternalWriterFactReference,
        );

        assert_eq!(
            decide_c2_categorical_fact_consumption(&attempt),
            C2CategoricalFactConsumptionDecision::AllowInternalWriterFactReference {
                source_fact,
                mutation_fact,
            }
        );
    }
}

#[test]
fn wrong_source_mutation_pair_fails_closed() {
    for (source_fact, expected) in accepted_mappings() {
        for requested in accepted_mutation_facts() {
            if requested == expected {
                continue;
            }

            let attempt = attempt(
                source_fact,
                requested,
                C2CategoricalFactConsumptionTarget::InternalWriterFactReference,
            );

            assert_eq!(
                decide_c2_categorical_fact_consumption(&attempt),
                C2CategoricalFactConsumptionDecision::Reject(
                    C2CategoricalFactConsumptionRejection::SourceMutationMismatch {
                        source: source_fact,
                        requested,
                        expected,
                    }
                )
            );
        }
    }
}

#[test]
fn blocked_consumption_targets_fail_closed_for_all_c2_facts() {
    for (source_fact, mutation_fact) in accepted_mappings() {
        for target in blocked_targets() {
            let attempt = attempt(source_fact, mutation_fact, target);

            assert_eq!(
                decide_c2_categorical_fact_consumption(&attempt),
                C2CategoricalFactConsumptionDecision::Reject(
                    C2CategoricalFactConsumptionRejection::BlockedConsumptionTarget { target }
                ),
                "target {} must remain blocked for {} / {}",
                target.as_str(),
                source_fact.as_str(),
                mutation_fact.as_str(),
            );
        }
    }
}

#[test]
fn no_effect_freeze_narrowing_and_recovery_do_not_escape_to_external_effects() {
    for (source_fact, mutation_fact) in [
        (
            C2BoundedPromiseReliabilitySourceFact::ValidExcusedExit,
            C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::SourceScopeLimitedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityNarrowing,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::FreezeOrNarrowingReversedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityRecovery,
        ),
    ] {
        for target in [
            C2CategoricalFactConsumptionTarget::NumericSocialTrustScore,
            C2CategoricalFactConsumptionTarget::PublicSocialTrustDisplay,
            C2CategoricalFactConsumptionTarget::RecommendationBoost,
            C2CategoricalFactConsumptionTarget::ContactUnlock,
            C2CategoricalFactConsumptionTarget::RelationshipDepthFact,
            C2CategoricalFactConsumptionTarget::ProjectionRefresh,
        ] {
            let attempt = attempt(source_fact, mutation_fact, target);

            assert_eq!(
                decide_c2_categorical_fact_consumption(&attempt),
                C2CategoricalFactConsumptionDecision::Reject(
                    C2CategoricalFactConsumptionRejection::BlockedConsumptionTarget { target }
                )
            );
        }
    }
}

#[test]
fn projection_only_authority_fails_closed_even_for_internal_reference() {
    let mut attempt = attempt(
        C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
        C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        C2CategoricalFactConsumptionTarget::InternalWriterFactReference,
    );
    attempt.authority_posture = SocialTrustAuthorityPosture::ProjectionOnly;

    assert_eq!(
        decide_c2_categorical_fact_consumption(&attempt),
        C2CategoricalFactConsumptionDecision::Reject(
            C2CategoricalFactConsumptionRejection::ProjectionOnlyAuthority
        )
    );
}

fn attempt(
    source_fact: C2BoundedPromiseReliabilitySourceFact,
    mutation_fact: C2BoundedPromiseReliabilityMutationFact,
    target: C2CategoricalFactConsumptionTarget,
) -> C2CategoricalFactConsumptionAttempt {
    C2CategoricalFactConsumptionAttempt {
        source_fact,
        mutation_fact,
        target,
        authority_posture: SocialTrustAuthorityPosture::WriterTruthOnly,
    }
}

fn accepted_mappings() -> Vec<(
    C2BoundedPromiseReliabilitySourceFact,
    C2BoundedPromiseReliabilityMutationFact,
)> {
    vec![
        (
            C2BoundedPromiseReliabilitySourceFact::CompletedAsAgreed,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::CompletedAfterGovernedReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityPositive,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::ValidExcusedExit,
            C2BoundedPromiseReliabilityMutationFact::NoEffectValidExcusedExit,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::SourceFactCorrected,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityCorrection,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::ReviewRequiredBoundaryIntersection,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityFreeze,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::SourceScopeLimitedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityNarrowing,
        ),
        (
            C2BoundedPromiseReliabilitySourceFact::FreezeOrNarrowingReversedAfterReview,
            C2BoundedPromiseReliabilityMutationFact::BoundedPromiseReliabilityRecovery,
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

fn blocked_targets() -> Vec<C2CategoricalFactConsumptionTarget> {
    vec![
        C2CategoricalFactConsumptionTarget::NumericSocialTrustScore,
        C2CategoricalFactConsumptionTarget::SocialTrustScoreDelta,
        C2CategoricalFactConsumptionTarget::SocialTrustWeight,
        C2CategoricalFactConsumptionTarget::SocialTrustRank,
        C2CategoricalFactConsumptionTarget::SocialTrustDisplayLevel,
        C2CategoricalFactConsumptionTarget::SocialTrustPublicLevel,
        C2CategoricalFactConsumptionTarget::PublicSocialTrustDisplay,
        C2CategoricalFactConsumptionTarget::RecoveryCeiling,
        C2CategoricalFactConsumptionTarget::DiscoveryPriority,
        C2CategoricalFactConsumptionTarget::RecommendationBoost,
        C2CategoricalFactConsumptionTarget::ContactUnlock,
        C2CategoricalFactConsumptionTarget::RoomTransition,
        C2CategoricalFactConsumptionTarget::SettlementProgression,
        C2CategoricalFactConsumptionTarget::PromiseRuntimeOutcome,
        C2CategoricalFactConsumptionTarget::ProofRuntimeOutcome,
        C2CategoricalFactConsumptionTarget::RelationshipDepthFact,
        C2CategoricalFactConsumptionTarget::ProjectionRefresh,
        C2CategoricalFactConsumptionTarget::PublicApiResponse,
        C2CategoricalFactConsumptionTarget::MobileUiState,
    ]
}
