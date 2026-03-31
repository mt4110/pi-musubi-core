//! MUSUBI core-domain crate.
//! Owns neutral account-adjacent identifiers only.
//! Must not own realm topology, settlement logic, or app/runtime wiring.
//! See `apps/backend/docs/package_boundaries.md`.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OrdinaryAccountId(String);

impl OrdinaryAccountId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ControlledExceptionalAccountId(String);

impl ControlledExceptionalAccountId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}
