//! MUSUBI orchestration crate.
//! Owns transactional outbox, durable command inbox, retry, quarantine, and
//! pruning behavior for later M1 work.
//! Must not own ledger truth, provider adapters, or app/runtime wiring.
//! See `apps/backend/docs/package_boundaries.md`.

mod error;
mod model;
mod policy;
mod postgres;
mod runtime;
mod store;

pub use error::OrchestrationError;
pub use model::{
    ArchivedCommandInboxEntry, ArchivedOutboxMessage, AuthoritativeChange, ClaimedOutboxMessage,
    CommandBeginOutcome, CommandCompletion, CommandEnvelope, CommandInboxEntry, CommandInboxStatus,
    CommandKey, ConsumeOutcome, DeliveryOutcome, DeliveryReceipt, ExternalIdempotencyKey,
    NewOutboxMessage, OutboxAttempt, OutboxDeliveryStatus, OutboxMessage, ProcessingFailure,
    PruneOutcome, QuarantineReason,
};
pub use policy::{RetentionPolicy, RetryClass, RetryPolicy, SchemaCompatibilityPolicy};
pub use postgres::PostgresOrchestrationStore;
pub use runtime::OrchestrationRuntime;
pub use store::{InMemoryOrchestrationStore, OrchestrationStore, WriterReadSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrchestrationSurface {
    TransactionalOutbox,
    DurableCommandInbox,
}

impl OrchestrationSurface {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransactionalOutbox => "transactional-outbox",
            Self::DurableCommandInbox => "durable-command-inbox",
        }
    }
}
