//! MUSUBI realm-domain crate.
//! Owns `Server`, `Realm`, `Citadel`, and `Pool` boundary concepts.
//! Must not own settlement logic, DB schema, or app/runtime wiring.
//! See `apps/backend/docs/package_boundaries.md`.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RealmId(String);

impl RealmId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerAlias(String);

impl ServerAlias {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CitadelId(String);

impl CitadelId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PoolId(String);

impl PoolId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealmClass {
    Shared,
    Dedicated,
    External,
}

impl RealmClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Dedicated => "dedicated",
            Self::External => "external",
        }
    }
}
