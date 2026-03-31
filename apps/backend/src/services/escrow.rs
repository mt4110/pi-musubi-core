use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::SharedState;

#[derive(Debug, Clone, Serialize)]
pub struct EscrowRecord {
    pub payment_id: String,
    pub payer_pi_uid: String,
    pub target_user_id: String,
    pub amount_pi: f64,
    pub txid: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub status: EscrowStatus,
}

#[derive(Debug, Clone, Copy, Serialize)]
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

#[derive(Debug)]
pub struct EscrowFundingInput {
    pub payment_id: String,
    pub payer_pi_uid: String,
    pub target_user_id: String,
    pub amount_pi: f64,
    pub txid: Option<String>,
    pub callback_status: String,
}

pub async fn fund_escrow(state: &SharedState, input: EscrowFundingInput) -> EscrowRecord {
    let record = EscrowRecord {
        payment_id: input.payment_id,
        payer_pi_uid: input.payer_pi_uid,
        target_user_id: input.target_user_id,
        amount_pi: input.amount_pi,
        txid: input.txid,
        updated_at: Utc::now(),
        status: EscrowStatus::Funded,
    };

    println!(
        "escrow funded: target_user_id={}, payer_pi_uid={}, payment_id={}, amount_pi={}, callback_status={}",
        record.target_user_id,
        record.payer_pi_uid,
        record.payment_id,
        record.amount_pi,
        input.callback_status,
    );

    state
        .escrows
        .write()
        .await
        .insert(record.target_user_id.clone(), record.clone());

    record
}
