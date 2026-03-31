use chrono::{DateTime, Utc};
use musubi_settlement_domain::EscrowStatus;
use serde::{Serialize, Serializer};

use crate::SharedState;

/// Temporary PoC escrow glue.
///
/// The storage shape and callback-oriented inputs stay in the app crate for now
/// because they still reflect in-memory PoC behavior rather than lawful domain
/// truth. Pure settlement concepts live in `musubi_settlement_domain`.
#[derive(Debug, Clone, Serialize)]
pub struct EscrowRecord {
    pub payment_id: String,
    pub payer_pi_uid: String,
    pub target_user_id: String,
    pub amount_pi: f64,
    pub txid: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_escrow_status")]
    pub status: EscrowStatus,
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

fn serialize_escrow_status<S>(status: &EscrowStatus, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(status.as_str())
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
