use chrono::{DateTime, Utc};
use musubi_social_trust_domain::{ProposedSocialTrustMutationAttempt, SocialTrustIntakeDecision};

#[derive(Debug)]
pub enum SocialTrustIntakePersistenceError {
    BadRequest(String),
    IdempotencyConflict {
        message: String,
        existing_attempt_id: String,
    },
    Database {
        message: String,
        code: Option<String>,
        constraint: Option<String>,
        retryable: bool,
    },
    Internal(String),
}

impl SocialTrustIntakePersistenceError {
    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::IdempotencyConflict { message, .. }
            | Self::Database { message, .. }
            | Self::Internal(message) => message,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordSocialTrustIntakeAttemptInput {
    pub subject_account_id: String,
    pub attempt: ProposedSocialTrustMutationAttempt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialTrustIntakeReplayStatus {
    Inserted,
    ReplayedIdentical,
}

#[derive(Clone, Debug)]
pub enum SocialTrustIntakePersistenceOutcome {
    Recorded(SocialTrustIntakeSnapshot),
    RejectedBeforePersistence { decision: SocialTrustIntakeDecision },
}

#[derive(Clone, Debug)]
pub struct SocialTrustIntakeSnapshot {
    pub attempt_id: String,
    pub intake_decision_id: String,
    pub subject_account_id: String,
    pub source_category: String,
    pub decision_kind: String,
    pub rejection_reason_code: Option<String>,
    pub request_payload_hash: String,
    pub decision_payload_hash: String,
    pub replay_status: SocialTrustIntakeReplayStatus,
    pub created_at: DateTime<Utc>,
}
