//! MUSUBI settlement-domain crate.
//! Owns pure settlement-facing domain concepts only.
//! Must not own provider I/O, DB persistence, or app/runtime wiring.
//! See `apps/backend/docs/package_boundaries.md`.

use musubi_core_domain::OrdinaryAccountId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PromiseId(String);

impl PromiseId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SettlementCaseId(String);

impl SettlementCaseId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaymentReference(String);

impl PaymentReference {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowStatus {
    Funded,
}

impl EscrowStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Funded => "Funded",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromiseParties {
    pub initiator_account_id: OrdinaryAccountId,
    pub counterparty_account_id: OrdinaryAccountId,
}
