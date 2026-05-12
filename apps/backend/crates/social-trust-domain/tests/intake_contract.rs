use musubi_social_trust_domain::{
    DurableIdempotencyPosture, EvidencePosture, ForbiddenSocialTrustSourceCategory,
    ProposedSocialTrustMutationAttempt, ReasonFactReference, RetentionClassReference,
    RetentionPosture, ReviewabilityPosture, SocialTrustAuthorityPosture, SocialTrustIntakeDecision,
    SocialTrustIntakeRejection, SocialTrustMutationAttemptIdempotencyKey,
    SocialTrustSourceCategory, WriterSourceReference, decide_social_trust_intake,
};

#[test]
fn forbidden_sources_fail_closed() {
    for source in forbidden_sources() {
        let mut attempt = complete_attempt();
        attempt.source_category = SocialTrustSourceCategory::Forbidden(source);

        let decision = decide_social_trust_intake(&attempt);

        assert_eq!(
            decision,
            SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::ForbiddenSource {
                source
            })
        );
    }
}

#[test]
fn unknown_source_category_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.source_category = SocialTrustSourceCategory::Unknown;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::UnknownSourceCategory)
    );
}

#[test]
fn missing_writer_source_reference_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.writer_source_reference = None;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingWriterSourceReference)
    );
}

#[test]
fn blank_writer_source_reference_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.writer_source_reference = Some(WriterSourceReference::new("  "));

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingWriterSourceReference)
    );
}

#[test]
fn missing_idempotency_posture_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.idempotency_posture = None;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingIdempotencyPosture)
    );
}

#[test]
fn blank_idempotency_key_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.idempotency_posture = Some(DurableIdempotencyPosture::DurableDedupeKey(
        SocialTrustMutationAttemptIdempotencyKey::new(""),
    ));

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingIdempotencyPosture)
    );
}

#[test]
fn missing_reason_fact_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.reason_fact = None;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingReasonFact)
    );
}

#[test]
fn blank_reason_fact_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.reason_fact = Some(ReasonFactReference::new(" "));

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingReasonFact)
    );
}

#[test]
fn missing_evidence_posture_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.evidence_posture = None;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingEvidencePosture)
    );
}

#[test]
fn missing_reviewability_posture_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.reviewability_posture = None;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingReviewabilityPosture)
    );
}

#[test]
fn missing_retention_posture_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.retention_posture = None;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingRetentionPosture)
    );
}

#[test]
fn blank_retention_class_reference_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.retention_posture = Some(RetentionPosture::Classified(RetentionClassReference::new(
        "\t",
    )));

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::MissingRetentionPosture)
    );
}

#[test]
fn projection_only_posture_fails_closed() {
    let mut attempt = complete_attempt();
    attempt.authority_posture = SocialTrustAuthorityPosture::ProjectionOnly;

    let decision = decide_social_trust_intake(&attempt);

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::Reject(SocialTrustIntakeRejection::ProjectionOnlyAuthority)
    );
}

#[test]
fn complete_writer_source_candidate_never_mutates_social_trust_directly() {
    let decision = decide_social_trust_intake(&complete_attempt());

    assert_eq!(
        decision,
        SocialTrustIntakeDecision::CandidateForWriterPersistence
    );
}

fn complete_attempt() -> ProposedSocialTrustMutationAttempt {
    ProposedSocialTrustMutationAttempt {
        source_category: SocialTrustSourceCategory::WriterSourceCandidate,
        writer_source_reference: Some(WriterSourceReference::new("source-fact-1")),
        reason_fact: Some(ReasonFactReference::new("reason-fact-1")),
        idempotency_posture: Some(DurableIdempotencyPosture::DurableDedupeKey(
            SocialTrustMutationAttemptIdempotencyKey::new("dedupe-1"),
        )),
        evidence_posture: Some(EvidencePosture::Bounded),
        reviewability_posture: Some(ReviewabilityPosture::Reviewable),
        retention_posture: Some(RetentionPosture::Classified(RetentionClassReference::new(
            "retention-class-1",
        ))),
        authority_posture: SocialTrustAuthorityPosture::WriterTruthOnly,
    }
}

fn forbidden_sources() -> Vec<ForbiddenSocialTrustSourceCategory> {
    vec![
        ForbiddenSocialTrustSourceCategory::ProjectionState,
        ForbiddenSocialTrustSourceCategory::AnalyticsState,
        ForbiddenSocialTrustSourceCategory::ModelOutput,
        ForbiddenSocialTrustSourceCategory::ObservabilityState,
        ForbiddenSocialTrustSourceCategory::ClientState,
        ForbiddenSocialTrustSourceCategory::FrontendState,
        ForbiddenSocialTrustSourceCategory::PaymentAmount,
        ForbiddenSocialTrustSourceCategory::PaymentFrequency,
        ForbiddenSocialTrustSourceCategory::SupportAmountOrStatus,
        ForbiddenSocialTrustSourceCategory::TokenHoldings,
        ForbiddenSocialTrustSourceCategory::Popularity,
        ForbiddenSocialTrustSourceCategory::FollowerCount,
        ForbiddenSocialTrustSourceCategory::ReplySpeed,
        ForbiddenSocialTrustSourceCategory::DwellTime,
        ForbiddenSocialTrustSourceCategory::Tenure,
        ForbiddenSocialTrustSourceCategory::RomanticDesirability,
        ForbiddenSocialTrustSourceCategory::Engagement,
        ForbiddenSocialTrustSourceCategory::EngagementTelemetry,
        ForbiddenSocialTrustSourceCategory::RecommendationState,
        ForbiddenSocialTrustSourceCategory::DiscoveryState,
        ForbiddenSocialTrustSourceCategory::DiscoveryRanking,
        ForbiddenSocialTrustSourceCategory::RelationshipDepth,
        ForbiddenSocialTrustSourceCategory::RoomProjection,
        ForbiddenSocialTrustSourceCategory::OperatorNotes,
        ForbiddenSocialTrustSourceCategory::SupportTickets,
        ForbiddenSocialTrustSourceCategory::IssueComments,
        ForbiddenSocialTrustSourceCategory::AntiAbuseMarkerExistence,
        ForbiddenSocialTrustSourceCategory::AgeAssurancePosture,
        ForbiddenSocialTrustSourceCategory::ProofCallbackAlone,
        ForbiddenSocialTrustSourceCategory::VendorCallbackAlone,
        ForbiddenSocialTrustSourceCategory::ControlledExceptionalAccountActivity,
        ForbiddenSocialTrustSourceCategory::ImplementationConvenience,
    ]
}
