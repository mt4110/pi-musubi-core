use musubi_core_domain::OrdinaryAccountId;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(PromiseId);
string_id!(PaymentReceiptId);
string_id!(SettlementCaseId);
string_id!(SettlementIntentId);
string_id!(SettlementSubmissionId);
string_id!(ObservationId);
string_id!(ProviderSubmissionId);
string_id!(InternalIdempotencyKey);
string_id!(ProviderIdempotencyKey);
string_id!(ProviderRef);
string_id!(ProviderTxHash);
string_id!(ProviderCallbackId);

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
