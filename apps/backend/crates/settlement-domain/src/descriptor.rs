use crate::{BackendKey, BackendPin, BackendVersion};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendDescriptor {
    pub backend_key: BackendKey,
    pub backend_version: BackendVersion,
    pub provider_family: ProviderFamily,
    pub execution_mode: ExecutionMode,
    pub capabilities: BackendCapabilities,
}

impl BackendDescriptor {
    pub fn pin(&self) -> BackendPin {
        BackendPin::new(self.backend_key.clone(), self.backend_version.clone())
    }

    pub fn supports(&self, capability: SettlementCapability) -> bool {
        self.capabilities.supports(capability)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendCapabilities(Vec<SettlementCapability>);

impl BackendCapabilities {
    pub fn new(capabilities: Vec<SettlementCapability>) -> Self {
        Self(capabilities)
    }

    pub fn supports(&self, capability: SettlementCapability) -> bool {
        self.0.contains(&capability)
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderFamily {
    PiNetwork,
    Other(String),
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    Synchronous,
    Asynchronous,
    Hybrid,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettlementCapability {
    ReceiptVerify,
    HoldValue,
    ReleaseValue,
    RefundValue,
    CompensateValue,
    AllocateTreasury,
    AttestExecution,
    ReconcileStatus,
    NormalizeCallback,
}
