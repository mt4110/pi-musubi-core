//! MUSUBI orchestration crate.
//! Owns coordination-boundary vocabulary for later M1 work.
//! Must not own ledger truth, provider adapters, or app/runtime wiring.
//! See `apps/backend/docs/package_boundaries.md`.

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
