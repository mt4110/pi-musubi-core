use chrono::{DateTime, Utc};
use musubi_settlement_domain::{
    NormalizedObservation, NormalizedObservationKind, ObservationConfidence,
};
use uuid::Uuid;

use super::{
    constants::{
        LEDGER_ACCOUNT_PROVIDER_CLEARING_INBOUND, LEDGER_ACCOUNT_USER_SECURED_FUNDS_LIABILITY,
        LEDGER_DIRECTION_CREDIT, LEDGER_DIRECTION_DEBIT,
    },
    state::{
        HappyRouteState, LedgerJournalRecord, LedgerPostingRecord, PromiseIntentRecord,
        SettlementCaseRecord, SettlementObservationRecord,
    },
};

pub(super) fn append_normalized_observations(
    store: &mut HappyRouteState,
    settlement_case_id: &str,
    settlement_submission_id: Option<&str>,
    observations: &[NormalizedObservation],
) {
    for observation in observations {
        store
            .settlement_observations
            .push(SettlementObservationRecord {
                observation_id: observation.observation_id.as_str().to_owned(),
                settlement_case_id: settlement_case_id.to_owned(),
                settlement_submission_id: settlement_submission_id.map(str::to_owned),
                observation_kind: normalized_observation_kind(observation.kind).to_owned(),
                confidence: observation_confidence(observation.confidence).to_owned(),
                provider_ref: observation
                    .provider_ref
                    .as_ref()
                    .map(|value| value.as_str().to_owned()),
                provider_tx_hash: observation
                    .provider_tx_hash
                    .as_ref()
                    .map(|value| value.as_str().to_owned()),
                observed_at: observation
                    .observed_at
                    .map(DateTime::<Utc>::from)
                    .unwrap_or_else(Utc::now),
            });
    }
}

pub(super) fn append_receipt_recognition_ledger(
    store: &mut HappyRouteState,
    settlement_case: &SettlementCaseRecord,
    promise_intent: &PromiseIntentRecord,
) -> String {
    let now = Utc::now();
    let journal_entry_id = Uuid::new_v4().to_string();
    let journal = LedgerJournalRecord {
        journal_entry_id: journal_entry_id.clone(),
        settlement_case_id: settlement_case.settlement_case_id.clone(),
        promise_intent_id: promise_intent.promise_intent_id.clone(),
        realm_id: settlement_case.realm_id.clone(),
        entry_kind: "receipt_recognized".to_owned(),
        effective_at: now,
        created_at: now,
    };
    store
        .ledger_journals_by_id
        .insert(journal_entry_id.clone(), journal);
    store.ledger_journal_order.push(journal_entry_id.clone());

    let debit_posting = LedgerPostingRecord {
        posting_id: Uuid::new_v4().to_string(),
        journal_entry_id: journal_entry_id.clone(),
        posting_order: 1,
        ledger_account_code: LEDGER_ACCOUNT_PROVIDER_CLEARING_INBOUND.to_owned(),
        account_id: None,
        direction: LEDGER_DIRECTION_DEBIT.to_owned(),
        amount: promise_intent.deposit_amount.clone(),
        created_at: now,
    };
    let credit_posting = LedgerPostingRecord {
        posting_id: Uuid::new_v4().to_string(),
        journal_entry_id: journal_entry_id.clone(),
        posting_order: 2,
        ledger_account_code: LEDGER_ACCOUNT_USER_SECURED_FUNDS_LIABILITY.to_owned(),
        account_id: Some(promise_intent.initiator_account_id.clone()),
        direction: LEDGER_DIRECTION_CREDIT.to_owned(),
        amount: promise_intent.deposit_amount.clone(),
        created_at: now,
    };
    store.ledger_postings.push(debit_posting);
    store.ledger_postings.push(credit_posting);

    journal_entry_id
}

fn normalized_observation_kind(kind: NormalizedObservationKind) -> &'static str {
    match kind {
        NormalizedObservationKind::ReceiptVerified => "receipt_verified",
        NormalizedObservationKind::SubmissionAccepted => "submission_accepted",
        NormalizedObservationKind::Pending => "pending",
        NormalizedObservationKind::Finalized => "finalized",
        NormalizedObservationKind::Failed => "failed",
        NormalizedObservationKind::Contradictory => "contradictory",
        NormalizedObservationKind::NotFound => "not_found",
        NormalizedObservationKind::CallbackNormalized => "callback_normalized",
        NormalizedObservationKind::Unknown => "unknown",
        _ => "unknown",
    }
}

fn observation_confidence(confidence: ObservationConfidence) -> &'static str {
    match confidence {
        ObservationConfidence::CryptographicProof => "cryptographic_proof",
        ObservationConfidence::ProviderConfirmed => "provider_confirmed",
        ObservationConfidence::HeuristicPending => "heuristic_pending",
        ObservationConfidence::Unknown => "unknown",
        _ => "unknown",
    }
}
