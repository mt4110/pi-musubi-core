use chrono::{DateTime, Utc};
use musubi_social_trust_domain::{
    C2BoundedPromiseReliabilityMutationDecision, ProposedC2BoundedPromiseReliabilityMutationFact,
};

#[derive(Debug)]
pub enum SocialTrustMutationPersistenceError {
    BadRequest(String),
    IdempotencyConflict {
        message: String,
        existing_source_reference_id: String,
    },
    Database {
        message: String,
        code: Option<String>,
        constraint: Option<String>,
        retryable: bool,
    },
    Internal(String),
}

impl SocialTrustMutationPersistenceError {
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
pub struct RecordC2BoundedPromiseReliabilityMutationFactInput {
    pub subject_account_id: String,
    pub realm_reference: Option<String>,
    pub proposal: ProposedC2BoundedPromiseReliabilityMutationFact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum C2BoundedPromiseReliabilityReplayStatus {
    Inserted,
    ReplayedIdentical,
}

#[derive(Clone, Debug)]
pub enum SocialTrustMutationPersistenceOutcome {
    Recorded(C2BoundedPromiseReliabilitySnapshot),
    RejectedBeforePersistence {
        decision: C2BoundedPromiseReliabilityMutationDecision,
    },
}

#[derive(Clone, Debug)]
pub struct C2BoundedPromiseReliabilitySnapshot {
    pub source_reference_id: String,
    pub mutation_fact_id: String,
    pub subject_account_id: String,
    pub source_fact_label: String,
    pub mutation_fact_label: String,
    pub mutation_direction: String,
    pub mutation_magnitude: String,
    pub request_payload_hash: String,
    pub decision_payload_hash: String,
    pub replay_status: C2BoundedPromiseReliabilityReplayStatus,
    pub created_at: DateTime<Utc>,
}
