#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementPrimaryPhase {
    PaymentExpected,
    PaymentConfirmed,
    HoldPending,
    HoldActive,
    FulfillmentOpen,
    ResolutionPending,
    Finalizable,
    Finalized,
    Void,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementResolutionKind {
    Release,
    Refund,
    Compensation,
    CancelNoSettlement,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettlementOverlay {
    Disputed,
    AppealOpen,
    LegalHold,
    SafetyFreeze,
    ManualReviewRequired,
    BackendUnknown,
    RealmQuarantineContext,
    TreasuryPending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementState {
    pub primary_phase: SettlementPrimaryPhase,
    pub resolution_kind: Option<SettlementResolutionKind>,
    pub overlays: Vec<SettlementOverlay>,
}

impl SettlementState {
    pub fn new(primary_phase: SettlementPrimaryPhase) -> Self {
        Self {
            primary_phase,
            resolution_kind: None,
            overlays: Vec::new(),
        }
    }
}
