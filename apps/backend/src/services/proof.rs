use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::SharedState;

use super::happy_route::HappyRouteError;

const CHALLENGE_TTL_SECONDS: i64 = 90;
const DISPLAY_CODE_WINDOW_SECONDS: i64 = 30;
const OPERATOR_PIN_TTL_SECONDS: i64 = 60;
const OPERATOR_PIN_RATE_LIMIT_WINDOW_SECONDS: i64 = 60;
const OPERATOR_PIN_RATE_LIMIT_PER_MINUTE: usize = 3;
const MAX_CHALLENGE_FAILED_ATTEMPTS: u8 = 3;
const PROOF_CHALLENGE_RETENTION_SECONDS: i64 = CHALLENGE_TTL_SECONDS * 2;
const PROOF_SUBMISSION_RETENTION_SECONDS: i64 = 15 * 60;
const MAX_ACTIVE_PROOF_CHALLENGES_PER_SUBJECT: usize = 4;
const MAX_RETAINED_PROOF_CHALLENGES: usize = MAX_RETAINED_PROOF_SUBMISSIONS;
const MAX_RETAINED_PROOF_SUBMISSIONS: usize = 1024;
const MAX_ACTIVE_PROOF_CHALLENGES: usize = MAX_RETAINED_PROOF_CHALLENGES;
const MAX_RETAINED_OPERATOR_PIN_AUDITS: usize = MAX_RETAINED_PROOF_CHALLENGES;

const KEY_STATUS_ACTIVE: &str = "active";
const KEY_STATUS_DRAINING: &str = "draining";
const KEY_STATUS_REVOKED: &str = "revoked";

const CHALLENGE_STATUS_ISSUED: &str = "issued";
const CHALLENGE_STATUS_CONSUMED: &str = "consumed";
const CHALLENGE_STATUS_EXPIRED: &str = "expired";
const CHALLENGE_STATUS_QUARANTINED: &str = "quarantined";

const FALLBACK_NONE: &str = "none";
const FALLBACK_OPERATOR_PIN: &str = "operator_pin";
const FALLBACK_UNSUPPORTED: &str = "unsupported";

const PROOF_STATUS_VERIFIED: &str = "verified";
const PROOF_STATUS_REJECTED: &str = "rejected";
const PROOF_STATUS_QUARANTINED: &str = "quarantined";

const REASON_VERIFIED: &str = "verified";
const REASON_MALFORMED: &str = "malformed";
const REASON_CHALLENGE_NOT_FOUND: &str = "challenge_not_found";
const REASON_EXPIRED: &str = "expired";
const REASON_REPLAY: &str = "replay";
const REASON_INVALID_CODE: &str = "invalid_code";
const REASON_KEY_REVOKED: &str = "key_revoked";
const REASON_VENUE_MISMATCH: &str = "venue_mismatch";
const REASON_SUBJECT_MISMATCH: &str = "subject_mismatch";
const REASON_OPERATOR_PIN_INVALID: &str = "operator_pin_invalid";
const REASON_RISK_FLAGGED: &str = "risk_flagged";
const REASON_ATTEMPT_LIMIT_EXCEEDED: &str = "attempt_limit_exceeded";
const REASON_UNSUPPORTED_FALLBACK_MODE: &str = "unsupported_fallback_mode";
const REASON_KEY_VERSION_MISMATCH: &str = "key_version_mismatch";

const RISK_INVALID_COARSE_LOCATION_HINT: &str = "invalid_coarse_location_hint";

type HmacSha256 = Hmac<Sha256>;

pub struct ProofState {
    server_secret: String,
    venue_key_versions: HashMap<(String, String, i32), VenueKeyVersionRecord>,
    active_key_version_by_venue: HashMap<(String, String), i32>,
    venue_challenges_by_id: HashMap<String, VenueChallengeRecord>,
    proof_submissions_by_id: HashMap<String, ProofSubmissionRecord>,
    proof_submission_id_by_replay_key: HashMap<String, String>,
    proof_verifications_by_id: HashMap<String, ProofVerificationRecord>,
    operator_pin_audits: Vec<OperatorPinAuditRecord>,
}

impl Default for ProofState {
    fn default() -> Self {
        Self::with_server_secret(server_secret_from_env())
    }
}

impl ProofState {
    fn with_server_secret(server_secret: String) -> Self {
        Self {
            server_secret,
            venue_key_versions: HashMap::new(),
            active_key_version_by_venue: HashMap::new(),
            venue_challenges_by_id: HashMap::new(),
            proof_submissions_by_id: HashMap::new(),
            proof_submission_id_by_replay_key: HashMap::new(),
            proof_verifications_by_id: HashMap::new(),
            operator_pin_audits: Vec::new(),
        }
    }

    #[cfg(test)]
    fn with_server_secret_for_test(server_secret: &str) -> Self {
        Self::with_server_secret(server_secret.to_owned())
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct VenueKeyVersionRecord {
    realm_id: String,
    venue_id: String,
    key_version: i32,
    secret_material: String,
    status: String,
    not_before: DateTime<Utc>,
    not_after: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct VenueChallengeRecord {
    challenge_id: String,
    subject_account_id: String,
    venue_id: String,
    realm_id: String,
    client_nonce_hash: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
    fallback_mode: String,
    operator_pin_hash: Option<[u8; 32]>,
    operator_pin_expires_at: Option<DateTime<Utc>>,
    operator_id: Option<String>,
    venue_key_version: i32,
    failed_attempt_count: u8,
    status: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct ProofSubmissionRecord {
    proof_submission_id: String,
    challenge_id: Option<String>,
    subject_account_id: String,
    replay_key: String,
    display_code_hash: Option<String>,
    received_at: DateTime<Utc>,
    observed_at_client: Option<DateTime<Utc>>,
    coarse_location_bucket: Option<String>,
    device_session_id_hash: Option<String>,
    fallback_mode: String,
    raw_payload_json: serde_json::Value,
    verification_status: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct ProofVerificationRecord {
    proof_verification_id: String,
    proof_submission_id: String,
    result: String,
    reason_code: String,
    risk_flags: Vec<String>,
    verified_at: DateTime<Utc>,
    operator_override_case_id: Option<String>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct OperatorPinAuditRecord {
    audit_id: String,
    challenge_id: String,
    operator_id: String,
    venue_id: String,
    realm_id: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    pin_hash: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct StartProofChallengeInput {
    pub subject_account_id: String,
    pub venue_id: String,
    pub realm_id: String,
    pub fallback_mode: String,
    pub operator_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StartProofChallengeOutcome {
    pub challenge_id: String,
    pub venue_id: String,
    pub realm_id: String,
    pub expires_at: DateTime<Utc>,
    pub client_nonce: String,
    pub allowed_fallback_mode: String,
    pub venue_key_version: i32,
    pub operator_pin_issued: bool,
}

#[derive(Clone, Debug)]
pub struct OperatorPinDelivery {
    pub challenge_id: String,
    pub operator_id: String,
    pub venue_id: String,
    pub realm_id: String,
    pub pin: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct StartProofChallengeServiceOutcome {
    pub client: StartProofChallengeOutcome,
    pub operator_delivery: Option<OperatorPinDelivery>,
}

#[derive(Clone, Debug)]
pub struct ProofEnvelopeInput {
    pub subject_account_id: String,
    pub challenge_id: Option<String>,
    pub venue_id: Option<String>,
    pub display_code: Option<String>,
    pub key_version: Option<i32>,
    pub client_nonce: Option<String>,
    pub observed_at_ms: Option<i64>,
    pub coarse_location_bucket: Option<String>,
    pub device_session_id: Option<String>,
    pub fallback_mode: Option<String>,
    pub operator_pin: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProofSubmissionOutcome {
    pub proof_submission_id: String,
    pub proof_verification_id: String,
    pub accepted: bool,
    pub verification_status: String,
    pub reason_code: Option<String>,
    pub risk_flags: Vec<String>,
    pub next_action: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubmissionFallbackMode {
    None,
    OperatorPin,
}

impl SubmissionFallbackMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => FALLBACK_NONE,
            Self::OperatorPin => FALLBACK_OPERATOR_PIN,
        }
    }
}

#[derive(Clone, Debug)]
struct SanitizedCoarseLocationBucket {
    stored_bucket: Option<String>,
    invalid: bool,
}

pub async fn start_proof_challenge(
    state: &SharedState,
    input: StartProofChallengeInput,
) -> Result<StartProofChallengeServiceOutcome, HappyRouteError> {
    let now = Utc::now();
    let venue_id = required_trimmed(input.venue_id, "venue_id is required")?;
    let realm_id = required_trimmed(input.realm_id, "realm_id is required")?;
    let subject_account_id =
        required_trimmed(input.subject_account_id, "subject_account_id is required")?;
    let fallback_mode = normalize_fallback_mode(input.fallback_mode.as_str())?;
    let mut store = state.proof.write().await;
    prune_ephemeral_proof_state(&mut store, now);
    if active_challenge_count_for_subject(&store, &subject_account_id, now)
        >= MAX_ACTIVE_PROOF_CHALLENGES_PER_SUBJECT
    {
        return Err(HappyRouteError::BadRequest(
            "active proof challenge limit exceeded".to_owned(),
        ));
    }
    if active_challenge_count(&store, now) >= MAX_ACTIVE_PROOF_CHALLENGES {
        return Err(HappyRouteError::BadRequest(
            "proof challenge capacity exceeded".to_owned(),
        ));
    }
    let key_version = ensure_active_venue_key(&mut store, &realm_id, &venue_id, now);
    let challenge_id = Uuid::new_v4().to_string();
    let client_nonce = Uuid::new_v4().to_string();
    let expires_at = now + Duration::seconds(CHALLENGE_TTL_SECONDS);

    let (operator_delivery, operator_pin_hash, operator_pin_expires_at, operator_id) =
        if fallback_mode == FALLBACK_OPERATOR_PIN {
            let operator_id = required_trimmed(
                input.operator_id.unwrap_or_default(),
                "operator_id is required for operator_pin fallback",
            )?;
            if operator_pin_issuance_count(&store, &realm_id, &venue_id, &operator_id, now)
                >= OPERATOR_PIN_RATE_LIMIT_PER_MINUTE
            {
                return Err(HappyRouteError::BadRequest(
                    "operator_pin fallback rate limit exceeded".to_owned(),
                ));
            }
            let operator_pin = random_numeric_code();
            let pin_hash = operator_pin_hash(&store.server_secret, &challenge_id, &operator_pin);
            let pin_expires_at = now + Duration::seconds(OPERATOR_PIN_TTL_SECONDS);
            store.operator_pin_audits.push(OperatorPinAuditRecord {
                audit_id: Uuid::new_v4().to_string(),
                challenge_id: challenge_id.clone(),
                operator_id: operator_id.clone(),
                venue_id: venue_id.clone(),
                realm_id: realm_id.clone(),
                issued_at: now,
                expires_at: pin_expires_at,
                pin_hash: pin_hash.clone(),
            });
            (
                Some(OperatorPinDelivery {
                    challenge_id: challenge_id.clone(),
                    operator_id: operator_id.clone(),
                    venue_id: venue_id.clone(),
                    realm_id: realm_id.clone(),
                    pin: operator_pin,
                    expires_at: pin_expires_at,
                }),
                Some(pin_hash),
                Some(pin_expires_at),
                Some(operator_id),
            )
        } else {
            (None, None, None, None)
        };

    store.venue_challenges_by_id.insert(
        challenge_id.clone(),
        VenueChallengeRecord {
            challenge_id: challenge_id.clone(),
            subject_account_id,
            venue_id: venue_id.clone(),
            realm_id: realm_id.clone(),
            client_nonce_hash: digest_parts(&["client-nonce", &challenge_id, &client_nonce]),
            issued_at: now,
            expires_at,
            consumed_at: None,
            fallback_mode: fallback_mode.clone(),
            operator_pin_hash,
            operator_pin_expires_at,
            operator_id,
            venue_key_version: key_version,
            failed_attempt_count: 0,
            status: CHALLENGE_STATUS_ISSUED.to_owned(),
        },
    );
    prune_ephemeral_proof_state(&mut store, now);

    Ok(StartProofChallengeServiceOutcome {
        client: StartProofChallengeOutcome {
            challenge_id,
            venue_id,
            realm_id,
            expires_at,
            client_nonce,
            allowed_fallback_mode: fallback_mode,
            venue_key_version: key_version,
            operator_pin_issued: operator_delivery.is_some(),
        },
        operator_delivery,
    })
}

pub async fn submit_proof_envelope(
    state: &SharedState,
    input: ProofEnvelopeInput,
) -> Result<ProofSubmissionOutcome, HappyRouteError> {
    submit_proof_envelope_at(state, input, Utc::now()).await
}

async fn submit_proof_envelope_at(
    state: &SharedState,
    input: ProofEnvelopeInput,
    received_at: DateTime<Utc>,
) -> Result<ProofSubmissionOutcome, HappyRouteError> {
    let mut store = state.proof.write().await;
    prune_ephemeral_proof_state(&mut store, received_at);
    let replay_key = replay_key(&store.server_secret, &input);
    let replayed_submission_id = store
        .proof_submission_id_by_replay_key
        .get(&replay_key)
        .cloned();
    let display_code_hash = input
        .display_code
        .as_deref()
        .map(|value| server_keyed_display_code_hash(&store.server_secret, value));
    let observed_at_client = input
        .observed_at_ms
        .and_then(DateTime::<Utc>::from_timestamp_millis);
    let sanitized_location =
        sanitize_coarse_location_bucket(input.coarse_location_bucket.as_deref());
    let device_session_id_hash = input
        .device_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| server_keyed_device_session_id_hash(&store.server_secret, value));
    let parsed_fallback_mode = parse_submission_fallback_mode(input.fallback_mode.as_deref());
    let fallback_mode = parsed_fallback_mode
        .map(SubmissionFallbackMode::as_str)
        .unwrap_or(FALLBACK_UNSUPPORTED)
        .to_owned();
    let proof_submission_id = Uuid::new_v4().to_string();
    let mut risk_flags = risk_flags(&input, &sanitized_location, received_at);

    let verification = if let Some(fallback_mode) = parsed_fallback_mode {
        verify_envelope(
            &mut store,
            &input,
            fallback_mode,
            received_at,
            replayed_submission_id.as_deref(),
            &mut risk_flags,
        )
    } else {
        no_charge(rejected(REASON_UNSUPPORTED_FALLBACK_MODE))
    };
    let VerificationResult {
        mut decision,
        charge_attempt,
    } = verification;
    if charge_attempt && decision.status != PROOF_STATUS_VERIFIED {
        decision = record_failed_attempt(&mut store, input.challenge_id.as_deref(), decision);
    }
    let raw_payload_json = redacted_payload(&input, &display_code_hash, &sanitized_location);
    let submission = ProofSubmissionRecord {
        proof_submission_id: proof_submission_id.clone(),
        challenge_id: input
            .challenge_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        subject_account_id: input.subject_account_id.clone(),
        replay_key: replay_key.clone(),
        display_code_hash,
        received_at,
        observed_at_client,
        coarse_location_bucket: sanitized_location.stored_bucket,
        device_session_id_hash,
        fallback_mode,
        raw_payload_json,
        verification_status: decision.status.to_owned(),
    };
    store
        .proof_submission_id_by_replay_key
        .insert(replay_key, proof_submission_id.clone());
    store
        .proof_submissions_by_id
        .insert(proof_submission_id.clone(), submission);

    if decision.status == PROOF_STATUS_VERIFIED
        && let Some(challenge_id) = input.challenge_id.as_deref().map(str::trim)
        && let Some(challenge) = store.venue_challenges_by_id.get_mut(challenge_id)
    {
        challenge.consumed_at = Some(received_at);
        challenge.status = CHALLENGE_STATUS_CONSUMED.to_owned();
    }

    let proof_verification_id = Uuid::new_v4().to_string();
    store.proof_verifications_by_id.insert(
        proof_verification_id.clone(),
        ProofVerificationRecord {
            proof_verification_id: proof_verification_id.clone(),
            proof_submission_id: proof_submission_id.clone(),
            result: decision.status.to_owned(),
            reason_code: decision.reason_code.to_owned(),
            risk_flags: risk_flags.clone(),
            verified_at: received_at,
            operator_override_case_id: None,
        },
    );
    prune_ephemeral_proof_state(&mut store, received_at);

    Ok(ProofSubmissionOutcome {
        proof_submission_id,
        proof_verification_id,
        accepted: decision.status == PROOF_STATUS_VERIFIED,
        verification_status: decision.status.to_owned(),
        reason_code: if decision.reason_code == REASON_VERIFIED {
            None
        } else {
            Some(decision.reason_code.to_owned())
        },
        risk_flags,
        next_action: decision.next_action.map(str::to_owned),
    })
}

struct VerificationDecision {
    status: &'static str,
    reason_code: &'static str,
    next_action: Option<&'static str>,
}

struct VerificationResult {
    decision: VerificationDecision,
    charge_attempt: bool,
}

fn verify_envelope(
    store: &mut ProofState,
    input: &ProofEnvelopeInput,
    fallback_mode: SubmissionFallbackMode,
    received_at: DateTime<Utc>,
    replayed_submission_id: Option<&str>,
    risk_flags: &mut Vec<String>,
) -> VerificationResult {
    if replayed_submission_id.is_some() {
        return no_charge(rejected(REASON_REPLAY));
    }

    let Some(challenge_id) = trimmed_optional(input.challenge_id.as_deref()) else {
        return no_charge(rejected(REASON_MALFORMED));
    };
    let Some(venue_id) = trimmed_optional(input.venue_id.as_deref()) else {
        return no_charge(rejected(REASON_MALFORMED));
    };
    let Some(client_nonce) = trimmed_optional(input.client_nonce.as_deref()) else {
        return no_charge(rejected(REASON_MALFORMED));
    };
    let Some(challenge) = store.venue_challenges_by_id.get(&challenge_id).cloned() else {
        return no_charge(rejected(REASON_CHALLENGE_NOT_FOUND));
    };
    if challenge.subject_account_id != input.subject_account_id {
        return no_charge(quarantined(REASON_SUBJECT_MISMATCH));
    }
    if challenge.venue_id != venue_id {
        return no_charge(quarantined(REASON_VENUE_MISMATCH));
    }
    if challenge.consumed_at.is_some() {
        return no_charge(rejected(REASON_REPLAY));
    }
    if challenge.status == CHALLENGE_STATUS_QUARANTINED
        || challenge.failed_attempt_count >= MAX_CHALLENGE_FAILED_ATTEMPTS
    {
        return no_charge(quarantined(REASON_ATTEMPT_LIMIT_EXCEEDED));
    }
    if received_at > challenge.expires_at {
        if let Some(existing) = store.venue_challenges_by_id.get_mut(&challenge_id) {
            existing.status = CHALLENGE_STATUS_EXPIRED.to_owned();
        }
        return no_charge(rejected(REASON_EXPIRED));
    }
    if challenge.client_nonce_hash != digest_parts(&["client-nonce", &challenge_id, &client_nonce])
    {
        return no_charge(rejected(REASON_MALFORMED));
    }
    if risk_requires_quarantine(risk_flags) {
        return no_charge(quarantined(REASON_RISK_FLAGGED));
    }

    if fallback_mode == SubmissionFallbackMode::OperatorPin {
        return verify_operator_pin(
            &store.server_secret,
            &challenge,
            input,
            received_at,
            risk_flags,
        );
    }

    let Some(display_code) = trimmed_optional(input.display_code.as_deref()) else {
        return no_charge(rejected(REASON_MALFORMED));
    };
    if input
        .key_version
        .is_some_and(|submitted| submitted != challenge.venue_key_version)
    {
        return no_charge(rejected(REASON_KEY_VERSION_MISMATCH));
    }
    let Some(key) = store
        .venue_key_versions
        .get(&(
            challenge.realm_id.clone(),
            venue_id.clone(),
            challenge.venue_key_version,
        ))
        .cloned()
    else {
        return no_charge(rejected(REASON_INVALID_CODE));
    };
    if key.status == KEY_STATUS_REVOKED {
        return no_charge(rejected(REASON_KEY_REVOKED));
    }
    if key.status != KEY_STATUS_ACTIVE && key.status != KEY_STATUS_DRAINING {
        return no_charge(rejected(REASON_INVALID_CODE));
    }
    if key.not_before > received_at
        || key
            .not_after
            .is_some_and(|not_after| received_at > not_after)
    {
        return no_charge(rejected(REASON_INVALID_CODE));
    }
    if !display_code_valid_for_key(&key, &display_code, received_at) {
        return charge(rejected(REASON_INVALID_CODE));
    }

    no_charge(verified())
}

fn verify_operator_pin(
    server_secret: &str,
    challenge: &VenueChallengeRecord,
    input: &ProofEnvelopeInput,
    received_at: DateTime<Utc>,
    risk_flags: &mut Vec<String>,
) -> VerificationResult {
    if challenge.fallback_mode != FALLBACK_OPERATOR_PIN {
        return no_charge(rejected(REASON_OPERATOR_PIN_INVALID));
    }
    let Some(pin) = trimmed_optional(input.operator_pin.as_deref()) else {
        return no_charge(rejected(REASON_OPERATOR_PIN_INVALID));
    };
    if challenge
        .operator_pin_expires_at
        .is_none_or(|expires_at| received_at > expires_at)
    {
        return no_charge(rejected(REASON_EXPIRED));
    }
    let Some(expected_pin_hash) = challenge.operator_pin_hash.as_ref() else {
        return no_charge(rejected(REASON_OPERATOR_PIN_INVALID));
    };
    if !hmac_sha256_matches(
        server_secret.as_bytes(),
        &["operator-pin", &challenge.challenge_id, &pin],
        expected_pin_hash,
    ) {
        return charge(rejected(REASON_OPERATOR_PIN_INVALID));
    }
    risk_flags.push("operator_fallback".to_owned());
    no_charge(verified())
}

fn no_charge(decision: VerificationDecision) -> VerificationResult {
    VerificationResult {
        decision,
        charge_attempt: false,
    }
}

fn charge(decision: VerificationDecision) -> VerificationResult {
    VerificationResult {
        decision,
        charge_attempt: true,
    }
}

fn verified() -> VerificationDecision {
    VerificationDecision {
        status: PROOF_STATUS_VERIFIED,
        reason_code: REASON_VERIFIED,
        next_action: None,
    }
}

fn rejected(reason_code: &'static str) -> VerificationDecision {
    VerificationDecision {
        status: PROOF_STATUS_REJECTED,
        reason_code,
        next_action: Some("retry_proof"),
    }
}

fn quarantined(reason_code: &'static str) -> VerificationDecision {
    VerificationDecision {
        status: PROOF_STATUS_QUARANTINED,
        reason_code,
        next_action: Some("operator_review"),
    }
}

fn record_failed_attempt(
    store: &mut ProofState,
    challenge_id: Option<&str>,
    decision: VerificationDecision,
) -> VerificationDecision {
    let Some(challenge_id) = challenge_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return decision;
    };
    let Some(challenge) = store.venue_challenges_by_id.get_mut(challenge_id) else {
        return decision;
    };
    if challenge.status != CHALLENGE_STATUS_ISSUED {
        return decision;
    }

    challenge.failed_attempt_count = challenge.failed_attempt_count.saturating_add(1);
    if challenge.failed_attempt_count >= MAX_CHALLENGE_FAILED_ATTEMPTS {
        challenge.status = CHALLENGE_STATUS_QUARANTINED.to_owned();
        quarantined(REASON_ATTEMPT_LIMIT_EXCEEDED)
    } else {
        decision
    }
}

fn normalize_fallback_mode(value: &str) -> Result<String, HappyRouteError> {
    let normalized = value.trim();
    if normalized.is_empty() || normalized == FALLBACK_NONE {
        Ok(FALLBACK_NONE.to_owned())
    } else if normalized == FALLBACK_OPERATOR_PIN {
        Ok(FALLBACK_OPERATOR_PIN.to_owned())
    } else {
        Err(HappyRouteError::BadRequest(
            "fallback_mode must be none or operator_pin".to_owned(),
        ))
    }
}

fn parse_submission_fallback_mode(value: Option<&str>) -> Option<SubmissionFallbackMode> {
    let normalized = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(FALLBACK_NONE);
    if normalized == FALLBACK_NONE {
        Some(SubmissionFallbackMode::None)
    } else if normalized == FALLBACK_OPERATOR_PIN {
        Some(SubmissionFallbackMode::OperatorPin)
    } else {
        None
    }
}

fn required_trimmed(value: String, message: &str) -> Result<String, HappyRouteError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(HappyRouteError::BadRequest(message.to_owned()))
    } else {
        Ok(value)
    }
}

fn trimmed_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn server_secret_from_env() -> String {
    std::env::var("PROOF_MASTER_SECRET")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(server_random_secret)
}

fn server_random_secret() -> String {
    digest_parts(&[
        "proof-server-secret",
        &Uuid::new_v4().to_string(),
        &Uuid::new_v4().to_string(),
    ])
}

fn random_numeric_code() -> String {
    let digest = Sha256::digest(format!("pin:{}:{}", Uuid::new_v4(), Uuid::new_v4()).as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    let value = u64::from_be_bytes(bytes) % 1_000_000;
    format!("{value:06}")
}

fn operator_pin_hash(server_secret: &str, challenge_id: &str, pin: &str) -> [u8; 32] {
    hmac_sha256(
        server_secret.as_bytes(),
        &["operator-pin", challenge_id, pin],
    )
}

fn venue_secret_material(
    server_secret: &str,
    realm_id: &str,
    venue_id: &str,
    key_version: i32,
) -> String {
    hmac_sha256_hex(
        server_secret,
        &["venue-secret", realm_id, venue_id, &key_version.to_string()],
    )
}

fn ensure_active_venue_key(
    store: &mut ProofState,
    realm_id: &str,
    venue_id: &str,
    now: DateTime<Utc>,
) -> i32 {
    let active_key = (realm_id.to_owned(), venue_id.to_owned());
    if let Some(key_version) = store.active_key_version_by_venue.get(&active_key) {
        return *key_version;
    }
    let key_version = 1;
    store
        .active_key_version_by_venue
        .insert(active_key, key_version);
    store.venue_key_versions.insert(
        (realm_id.to_owned(), venue_id.to_owned(), key_version),
        VenueKeyVersionRecord {
            realm_id: realm_id.to_owned(),
            venue_id: venue_id.to_owned(),
            key_version,
            secret_material: venue_secret_material(
                &store.server_secret,
                realm_id,
                venue_id,
                key_version,
            ),
            status: KEY_STATUS_ACTIVE.to_owned(),
            not_before: now,
            not_after: None,
            created_at: now,
        },
    );
    key_version
}

fn display_code_valid_for_key(
    key: &VenueKeyVersionRecord,
    display_code: &str,
    received_at: DateTime<Utc>,
) -> bool {
    let normalized = display_code.trim().to_ascii_uppercase();
    venue_display_code_for_window(key, received_at) == normalized
        || venue_display_code_for_window(
            key,
            received_at - Duration::seconds(DISPLAY_CODE_WINDOW_SECONDS),
        ) == normalized
}

fn venue_display_code_for_window(key: &VenueKeyVersionRecord, at: DateTime<Utc>) -> String {
    let window = at.timestamp().div_euclid(DISPLAY_CODE_WINDOW_SECONDS);
    short_code_from_bytes(&hmac_sha256(
        key.secret_material.as_bytes(),
        &[
            "venue-display-code",
            key.realm_id.as_str(),
            key.venue_id.as_str(),
            &key.key_version.to_string(),
            &window.to_string(),
        ],
    ))
}

fn sanitize_coarse_location_bucket(value: Option<&str>) -> SanitizedCoarseLocationBucket {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return SanitizedCoarseLocationBucket {
            stored_bucket: None,
            invalid: false,
        };
    };
    let canonical = value.to_ascii_lowercase();
    let valid = canonical.len() <= 32
        && canonical
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && canonical.contains('-')
        && !canonical.contains("--")
        && canonical
            .split('-')
            .all(|part| !part.is_empty() && !part.chars().all(|ch| ch.is_ascii_digit()));

    SanitizedCoarseLocationBucket {
        stored_bucket: valid.then_some(canonical),
        invalid: !valid,
    }
}

fn short_code_from_bytes(digest: &[u8]) -> String {
    let mut encoded = String::with_capacity(8);
    for byte in digest.iter().take(4) {
        let _ = write!(&mut encoded, "{byte:02X}");
    }
    encoded.truncate(6);
    encoded
}

fn server_keyed_display_code_hash(server_secret: &str, display_code: &str) -> String {
    hmac_sha256_hex(
        server_secret,
        &[
            "display-code",
            display_code.trim().to_ascii_uppercase().as_str(),
        ],
    )
}

fn server_keyed_device_session_id_hash(server_secret: &str, device_session_id: &str) -> String {
    hmac_sha256_hex(server_secret, &["device-session", device_session_id.trim()])
}

fn replay_key(server_secret: &str, input: &ProofEnvelopeInput) -> String {
    let subject_account_id = input.subject_account_id.trim().to_owned();
    let challenge_id = canonical_optional(input.challenge_id.as_deref());
    let venue_id = canonical_optional(input.venue_id.as_deref());
    let display_code = input
        .display_code
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_uppercase();
    let key_version = input
        .key_version
        .map(|value| value.to_string())
        .unwrap_or_default();
    let client_nonce = canonical_optional(input.client_nonce.as_deref());
    let observed_at_ms = input
        .observed_at_ms
        .map(|value| value.to_string())
        .unwrap_or_default();
    let coarse_location_bucket = input
        .coarse_location_bucket
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let device_session_id = canonical_optional(input.device_session_id.as_deref());
    let fallback_mode = canonical_optional(input.fallback_mode.as_deref());
    let operator_pin = canonical_optional(input.operator_pin.as_deref());

    hmac_sha256_hex(
        server_secret,
        &[
            "proof-envelope",
            subject_account_id.as_str(),
            challenge_id.as_str(),
            venue_id.as_str(),
            display_code.as_str(),
            key_version.as_str(),
            client_nonce.as_str(),
            observed_at_ms.as_str(),
            coarse_location_bucket.as_str(),
            device_session_id.as_str(),
            fallback_mode.as_str(),
            operator_pin.as_str(),
        ],
    )
}

fn canonical_optional(value: Option<&str>) -> String {
    value.map(str::trim).unwrap_or_default().to_owned()
}

fn canonical_optional_json(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn redacted_payload(
    input: &ProofEnvelopeInput,
    display_code_hash: &Option<String>,
    sanitized_location: &SanitizedCoarseLocationBucket,
) -> serde_json::Value {
    serde_json::json!({
        "challenge_id": canonical_optional_json(input.challenge_id.as_deref()),
        "venue_id": canonical_optional_json(input.venue_id.as_deref()),
        "display_code_hash": display_code_hash.as_deref(),
        "key_version": input.key_version,
        "client_nonce_present": input.client_nonce.as_deref().is_some_and(|value| !value.trim().is_empty()),
        "observed_at_ms": input.observed_at_ms,
        "coarse_location_bucket": sanitized_location.stored_bucket.as_deref(),
        "coarse_location_hint_invalid": sanitized_location.invalid,
        "device_session_id_present": input.device_session_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
        "fallback_mode": parse_submission_fallback_mode(input.fallback_mode.as_deref())
            .map(SubmissionFallbackMode::as_str)
            .unwrap_or(FALLBACK_UNSUPPORTED),
        "operator_pin_present": input.operator_pin.as_deref().is_some_and(|value| !value.trim().is_empty()),
    })
}

fn prune_ephemeral_proof_state(store: &mut ProofState, now: DateTime<Utc>) {
    prune_venue_challenge_state(store, now);
    prune_venue_key_state(store);
    prune_operator_pin_audit_state(store, now);
    prune_proof_submission_state(store, now);
}

fn prune_venue_challenge_state(store: &mut ProofState, now: DateTime<Utc>) {
    let cutoff = now - Duration::seconds(PROOF_CHALLENGE_RETENTION_SECONDS);
    let mut expired_challenge_ids: HashSet<String> = store
        .venue_challenges_by_id
        .iter()
        .filter_map(|(challenge_id, challenge)| {
            (challenge_retention_anchor(challenge) < cutoff).then(|| challenge_id.clone())
        })
        .collect();

    let mut retained_challenges: Vec<(String, DateTime<Utc>)> = store
        .venue_challenges_by_id
        .iter()
        .filter(|(_, challenge)| challenge_retention_anchor(challenge) >= cutoff)
        .map(|(challenge_id, challenge)| (challenge_id.clone(), challenge.issued_at))
        .collect();
    let total_retained = retained_challenges.len();
    retained_challenges.retain(|(challenge_id, _)| {
        store
            .venue_challenges_by_id
            .get(challenge_id)
            .is_some_and(|challenge| !is_challenge_active(challenge, now))
    });
    retained_challenges.sort_by(|(left_id, left_issued_at), (right_id, right_issued_at)| {
        left_issued_at
            .cmp(right_issued_at)
            .then_with(|| left_id.cmp(right_id))
    });

    let overflow = total_retained.saturating_sub(MAX_RETAINED_PROOF_CHALLENGES);
    expired_challenge_ids.extend(
        retained_challenges
            .into_iter()
            .take(overflow)
            .map(|(challenge_id, _)| challenge_id),
    );

    if expired_challenge_ids.is_empty() {
        return;
    }

    store
        .venue_challenges_by_id
        .retain(|challenge_id, _| !expired_challenge_ids.contains(challenge_id));
}

fn challenge_retention_anchor(challenge: &VenueChallengeRecord) -> DateTime<Utc> {
    let mut anchor = challenge.expires_at;
    if let Some(consumed_at) = challenge.consumed_at
        && consumed_at > anchor
    {
        anchor = consumed_at;
    }
    if let Some(operator_pin_expires_at) = challenge.operator_pin_expires_at
        && operator_pin_expires_at > anchor
    {
        anchor = operator_pin_expires_at;
    }
    anchor
}

fn is_challenge_active(challenge: &VenueChallengeRecord, now: DateTime<Utc>) -> bool {
    challenge.status == CHALLENGE_STATUS_ISSUED
        && challenge.consumed_at.is_none()
        && now <= challenge.expires_at
}

fn active_challenge_count(store: &ProofState, now: DateTime<Utc>) -> usize {
    store
        .venue_challenges_by_id
        .values()
        .filter(|challenge| is_challenge_active(challenge, now))
        .count()
}

fn active_challenge_count_for_subject(
    store: &ProofState,
    subject_account_id: &str,
    now: DateTime<Utc>,
) -> usize {
    store
        .venue_challenges_by_id
        .values()
        .filter(|challenge| {
            challenge.subject_account_id == subject_account_id
                && is_challenge_active(challenge, now)
        })
        .count()
}

fn prune_venue_key_state(store: &mut ProofState) {
    let retained_venues: HashSet<(String, String)> = store
        .venue_challenges_by_id
        .values()
        .map(|challenge| (challenge.realm_id.clone(), challenge.venue_id.clone()))
        .collect();

    let mut retained_key_ids: HashSet<(String, String, i32)> = store
        .venue_challenges_by_id
        .values()
        .map(|challenge| {
            (
                challenge.realm_id.clone(),
                challenge.venue_id.clone(),
                challenge.venue_key_version,
            )
        })
        .collect();

    for venue in &retained_venues {
        if let Some(active_key_version) = store.active_key_version_by_venue.get(venue) {
            retained_key_ids.insert((venue.0.clone(), venue.1.clone(), *active_key_version));
        }
    }

    store
        .venue_key_versions
        .retain(|key_id, _| retained_key_ids.contains(key_id));

    let previous_active = std::mem::take(&mut store.active_key_version_by_venue);
    let mut rebuilt_active = HashMap::new();
    for venue in retained_venues {
        if let Some(active_key_version) = previous_active.get(&venue).copied()
            && store.venue_key_versions.contains_key(&(
                venue.0.clone(),
                venue.1.clone(),
                active_key_version,
            ))
        {
            rebuilt_active.insert(venue, active_key_version);
            continue;
        }

        if let Some(fallback_key_version) = store
            .venue_key_versions
            .keys()
            .filter_map(|(realm_id, venue_id, key_version)| {
                (realm_id == &venue.0 && venue_id == &venue.1).then_some(*key_version)
            })
            .max()
        {
            rebuilt_active.insert(venue, fallback_key_version);
        }
    }
    store.active_key_version_by_venue = rebuilt_active;
}

fn prune_operator_pin_audit_state(store: &mut ProofState, now: DateTime<Utc>) {
    let cutoff = now - Duration::seconds(OPERATOR_PIN_RATE_LIMIT_WINDOW_SECONDS);
    store
        .operator_pin_audits
        .retain(|audit| audit.issued_at >= cutoff && audit.issued_at <= now);
    store.operator_pin_audits.sort_by(|left, right| {
        left.issued_at
            .cmp(&right.issued_at)
            .then_with(|| left.audit_id.cmp(&right.audit_id))
    });

    let overflow = store
        .operator_pin_audits
        .len()
        .saturating_sub(MAX_RETAINED_OPERATOR_PIN_AUDITS);
    if overflow > 0 {
        store.operator_pin_audits.drain(0..overflow);
    }
}

fn prune_proof_submission_state(store: &mut ProofState, now: DateTime<Utc>) {
    let cutoff = now - Duration::seconds(PROOF_SUBMISSION_RETENTION_SECONDS);
    let mut expired_submission_ids: HashSet<String> = store
        .proof_submissions_by_id
        .iter()
        .filter_map(|(submission_id, submission)| {
            (submission.received_at < cutoff).then(|| submission_id.clone())
        })
        .collect();

    let mut retained_submissions: Vec<(String, DateTime<Utc>)> = store
        .proof_submissions_by_id
        .iter()
        .filter(|(_, submission)| submission.received_at >= cutoff)
        .map(|(submission_id, submission)| (submission_id.clone(), submission.received_at))
        .collect();
    retained_submissions.sort_by(
        |(left_id, left_received_at), (right_id, right_received_at)| {
            left_received_at
                .cmp(right_received_at)
                .then_with(|| left_id.cmp(right_id))
        },
    );

    let overflow = retained_submissions
        .len()
        .saturating_sub(MAX_RETAINED_PROOF_SUBMISSIONS);
    expired_submission_ids.extend(
        retained_submissions
            .into_iter()
            .take(overflow)
            .map(|(submission_id, _)| submission_id),
    );

    if expired_submission_ids.is_empty() {
        return;
    }

    store
        .proof_submissions_by_id
        .retain(|submission_id, _| !expired_submission_ids.contains(submission_id));
    store.proof_verifications_by_id.retain(|_, verification| {
        !expired_submission_ids.contains(&verification.proof_submission_id)
    });

    let mut retained_submissions: Vec<(&String, &ProofSubmissionRecord)> =
        store.proof_submissions_by_id.iter().collect();
    retained_submissions.sort_by(|(left_id, left_submission), (right_id, right_submission)| {
        left_submission
            .received_at
            .cmp(&right_submission.received_at)
            .then_with(|| left_id.cmp(right_id))
    });

    let mut replay_index = HashMap::new();
    for (submission_id, submission) in retained_submissions {
        replay_index.insert(submission.replay_key.clone(), submission_id.clone());
    }
    store.proof_submission_id_by_replay_key = replay_index;
}

fn risk_flags(
    input: &ProofEnvelopeInput,
    sanitized_location: &SanitizedCoarseLocationBucket,
    received_at: DateTime<Utc>,
) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(observed_at_ms) = input.observed_at_ms {
        if let Some(observed_at) = DateTime::<Utc>::from_timestamp_millis(observed_at_ms) {
            if (received_at - observed_at).num_seconds().abs() > 120 {
                flags.push("client_clock_skew_high".to_owned());
            }
        } else {
            flags.push("client_clock_invalid".to_owned());
        }
    }
    if sanitized_location.invalid {
        flags.push(RISK_INVALID_COARSE_LOCATION_HINT.to_owned());
    }
    if let Some(device_session_id) = input.device_session_id.as_deref()
        && device_session_id.trim().len() > 128
    {
        flags.push("device_hint_oversized".to_owned());
    }
    flags
}

fn risk_requires_quarantine(risk_flags: &[String]) -> bool {
    risk_flags.iter().any(|flag| {
        flag == "client_clock_invalid"
            || flag == "client_clock_skew_high"
            || flag == "device_hint_oversized"
            || flag == RISK_INVALID_COARSE_LOCATION_HINT
    })
}

fn operator_pin_issuance_count(
    store: &ProofState,
    realm_id: &str,
    venue_id: &str,
    operator_id: &str,
    now: DateTime<Utc>,
) -> usize {
    let since = now - Duration::seconds(OPERATOR_PIN_RATE_LIMIT_WINDOW_SECONDS);
    store
        .operator_pin_audits
        .iter()
        .filter(|audit| {
            audit.realm_id == realm_id
                && audit.venue_id == venue_id
                && audit.operator_id == operator_id
                && audit.issued_at >= since
                && audit.issued_at <= now
        })
        .count()
}

fn digest_parts(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    update_hasher_with_parts(&mut hasher, parts);
    hex_bytes(&hasher.finalize())
}

fn hmac_sha256_hex(key: &str, parts: &[&str]) -> String {
    hex_bytes(&hmac_sha256(key.as_bytes(), parts))
}

fn hmac_sha256(key: &[u8], parts: &[&str]) -> [u8; 32] {
    let mut mac = new_hmac_sha256(key);
    update_hmac_with_parts(&mut mac, parts);
    mac.finalize().into_bytes().into()
}

fn hmac_sha256_matches(key: &[u8], parts: &[&str], expected: &[u8]) -> bool {
    let mut mac = new_hmac_sha256(key);
    update_hmac_with_parts(&mut mac, parts);
    mac.verify_slice(expected).is_ok()
}

fn new_hmac_sha256(key: &[u8]) -> HmacSha256 {
    HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts arbitrary key length")
}

fn update_hmac_with_parts(mac: &mut HmacSha256, parts: &[&str]) {
    for part in parts {
        mac.update(part.len().to_string().as_bytes());
        mac.update(b":");
        mac.update(part.as_bytes());
        mac.update(b";");
    }
}

fn update_hasher_with_parts(hasher: &mut Sha256, parts: &[&str]) {
    for part in parts {
        hasher.update(part.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(part.as_bytes());
        hasher.update(b";");
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn valid_dynamic_venue_code_verifies_without_raw_gps_persistence() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-a", FALLBACK_NONE).await;
        let code = display_code(&state, "venue-a", challenge.venue_key_version, Utc::now()).await;

        let outcome = submit_proof_envelope(
            &state,
            valid_envelope(&challenge, code)
                .coarse_location_bucket("tokyo-shibuya")
                .device_session_id("ephemeral-device-session")
                .into(),
        )
        .await
        .expect("valid proof should verify");

        assert!(outcome.accepted);
        assert_eq!(outcome.verification_status, PROOF_STATUS_VERIFIED);
        let store = state.proof.read().await;
        let submission = store
            .proof_submissions_by_id
            .get(&outcome.proof_submission_id)
            .expect("proof submission must be retained");
        assert_eq!(
            submission.coarse_location_bucket.as_deref(),
            Some("tokyo-shibuya")
        );
        assert!(submission.device_session_id_hash.is_some());
        assert!(submission.raw_payload_json.get("raw_latitude").is_none());
        assert!(submission.raw_payload_json.get("raw_longitude").is_none());
        assert!(
            submission
                .raw_payload_json
                .get("display_code_hash")
                .and_then(|value| value.as_str())
                .is_some()
        );
    }

    #[tokio::test]
    async fn valid_coarse_location_bucket_is_canonicalized_before_recording() {
        let state = crate::new_state();
        let challenge =
            start_test_challenge(&state, "venue-canonical-location", FALLBACK_NONE).await;
        let code = display_code(
            &state,
            "venue-canonical-location",
            challenge.venue_key_version,
            Utc::now(),
        )
        .await;

        let outcome = submit_proof_envelope(
            &state,
            valid_envelope(&challenge, code)
                .coarse_location_bucket("Tokyo-Shibuya")
                .into(),
        )
        .await
        .expect("valid coarse location proof should produce a result");

        assert!(outcome.accepted);
        let store = state.proof.read().await;
        let submission = store
            .proof_submissions_by_id
            .get(&outcome.proof_submission_id)
            .expect("submission should be retained");
        assert_eq!(
            submission.coarse_location_bucket.as_deref(),
            Some("tokyo-shibuya")
        );
        assert_eq!(
            submission.raw_payload_json["coarse_location_bucket"],
            "tokyo-shibuya"
        );
    }

    #[tokio::test]
    async fn invalid_coarse_location_hints_are_dropped_before_recording() {
        let invalid_hints = [
            "35.6812,139.7671",
            "1-1-chiyoda-tokyo",
            "tokyo-shibuya-proof-location-value-that-is-too-long",
        ];

        for (index, invalid_hint) in invalid_hints.iter().enumerate() {
            let state = crate::new_state();
            let venue_id = format!("venue-invalid-location-{index}");
            let challenge = start_test_challenge(&state, &venue_id, FALLBACK_NONE).await;
            let code =
                display_code(&state, &venue_id, challenge.venue_key_version, Utc::now()).await;

            let outcome = submit_proof_envelope(
                &state,
                valid_envelope(&challenge, code)
                    .coarse_location_bucket(invalid_hint)
                    .into(),
            )
            .await
            .expect("invalid location proof should produce a result");

            assert!(!outcome.accepted);
            assert_eq!(outcome.verification_status, PROOF_STATUS_QUARANTINED);
            assert!(
                outcome
                    .risk_flags
                    .iter()
                    .any(|flag| flag == RISK_INVALID_COARSE_LOCATION_HINT)
            );
            let store = state.proof.read().await;
            let submission = store
                .proof_submissions_by_id
                .get(&outcome.proof_submission_id)
                .expect("submission should be retained");
            assert!(submission.coarse_location_bucket.is_none());
            assert!(submission.raw_payload_json["coarse_location_bucket"].is_null());
            assert_eq!(
                submission.raw_payload_json["coarse_location_hint_invalid"],
                true
            );
            assert!(
                !submission
                    .raw_payload_json
                    .to_string()
                    .contains(invalid_hint),
                "invalid location hint must not remain in redacted payload"
            );
        }
    }

    #[tokio::test]
    async fn unsupported_submission_fallback_mode_is_rejected_before_verification() {
        let state = crate::new_state();
        let challenge =
            start_test_challenge(&state, "venue-unsupported-fallback", FALLBACK_NONE).await;
        let code = display_code(
            &state,
            "venue-unsupported-fallback",
            challenge.venue_key_version,
            Utc::now(),
        )
        .await;

        let outcome = submit_proof_envelope(
            &state,
            valid_envelope(&challenge, code)
                .fallback_mode("operator-pni")
                .into(),
        )
        .await
        .expect("unsupported fallback mode should produce a result");

        assert!(!outcome.accepted);
        assert_eq!(outcome.verification_status, PROOF_STATUS_REJECTED);
        assert_eq!(
            outcome.reason_code.as_deref(),
            Some(REASON_UNSUPPORTED_FALLBACK_MODE)
        );
        let store = state.proof.read().await;
        let submission = store
            .proof_submissions_by_id
            .get(&outcome.proof_submission_id)
            .expect("submission should be retained");
        assert_eq!(submission.fallback_mode, FALLBACK_UNSUPPORTED);
        assert_eq!(submission.verification_status, PROOF_STATUS_REJECTED);
        assert!(
            !submission
                .raw_payload_json
                .to_string()
                .contains("operator-pni")
        );
    }

    #[tokio::test]
    async fn blank_submission_fallback_mode_uses_normal_flow() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-blank-fallback", FALLBACK_NONE).await;
        let code = display_code(
            &state,
            "venue-blank-fallback",
            challenge.venue_key_version,
            Utc::now(),
        )
        .await;

        let outcome = submit_proof_envelope(
            &state,
            valid_envelope(&challenge, code).fallback_mode("   ").into(),
        )
        .await
        .expect("blank fallback mode should use normal proof flow");

        assert!(outcome.accepted);
        let store = state.proof.read().await;
        let submission = store
            .proof_submissions_by_id
            .get(&outcome.proof_submission_id)
            .expect("submission should be retained");
        assert_eq!(submission.fallback_mode, FALLBACK_NONE);
    }

    #[tokio::test]
    async fn expired_challenge_is_rejected_by_server_time() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-expired", FALLBACK_NONE).await;
        let code = display_code(
            &state,
            "venue-expired",
            challenge.venue_key_version,
            Utc::now(),
        )
        .await;
        {
            let mut store = state.proof.write().await;
            let record = store
                .venue_challenges_by_id
                .get_mut(&challenge.challenge_id)
                .expect("challenge should exist");
            record.expires_at = Utc::now() - Duration::seconds(1);
        }

        let outcome = submit_proof_envelope(&state, valid_envelope(&challenge, code).into())
            .await
            .expect("expired proof should still produce a verification result");

        assert!(!outcome.accepted);
        assert_eq!(outcome.verification_status, PROOF_STATUS_REJECTED);
        assert_eq!(outcome.reason_code.as_deref(), Some(REASON_EXPIRED));
    }

    #[tokio::test]
    async fn replayed_envelope_is_rejected() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-replay", FALLBACK_NONE).await;
        let code = display_code(
            &state,
            "venue-replay",
            challenge.venue_key_version,
            Utc::now(),
        )
        .await;
        let envelope: ProofEnvelopeInput = valid_envelope(&challenge, code).into();

        let first = submit_proof_envelope(&state, envelope.clone())
            .await
            .expect("first proof should verify");
        assert!(first.accepted);
        let replay = submit_proof_envelope(&state, envelope)
            .await
            .expect("replay should produce a verification result");

        assert!(!replay.accepted);
        assert_eq!(replay.reason_code.as_deref(), Some(REASON_REPLAY));
    }

    #[test]
    fn same_envelope_gets_same_replay_key_under_same_server_secret() {
        let input = ProofEnvelopeInput {
            subject_account_id: "account-replay-key".to_owned(),
            challenge_id: Some("challenge-replay-key".to_owned()),
            venue_id: Some("venue-replay-key".to_owned()),
            display_code: Some("abc123".to_owned()),
            key_version: Some(7),
            client_nonce: Some("nonce-replay-key".to_owned()),
            observed_at_ms: Some(123456789),
            coarse_location_bucket: Some("Tokyo-Shibuya".to_owned()),
            device_session_id: Some("device-replay-key".to_owned()),
            fallback_mode: Some(FALLBACK_NONE.to_owned()),
            operator_pin: None,
        };

        assert_eq!(
            replay_key("server-secret-a", &input),
            replay_key("server-secret-a", &input)
        );
    }

    #[test]
    fn different_server_secret_changes_replay_key() {
        let input = ProofEnvelopeInput {
            subject_account_id: "account-replay-key".to_owned(),
            challenge_id: Some("challenge-replay-key".to_owned()),
            venue_id: Some("venue-replay-key".to_owned()),
            display_code: Some("123456".to_owned()),
            key_version: Some(1),
            client_nonce: Some("nonce-replay-key".to_owned()),
            observed_at_ms: Some(123456789),
            coarse_location_bucket: None,
            device_session_id: None,
            fallback_mode: Some(FALLBACK_NONE.to_owned()),
            operator_pin: Some("654321".to_owned()),
        };

        assert_ne!(
            replay_key("server-secret-a", &input),
            replay_key("server-secret-b", &input)
        );
    }

    #[test]
    fn different_subject_changes_replay_key() {
        let first = ProofEnvelopeInput {
            subject_account_id: "account-replay-key-a".to_owned(),
            challenge_id: Some("challenge-replay-key".to_owned()),
            venue_id: Some("venue-replay-key".to_owned()),
            display_code: Some("ABC123".to_owned()),
            key_version: Some(7),
            client_nonce: Some("nonce-replay-key".to_owned()),
            observed_at_ms: Some(123456789),
            coarse_location_bucket: Some("tokyo-shibuya".to_owned()),
            device_session_id: Some("device-replay-key".to_owned()),
            fallback_mode: Some(FALLBACK_NONE.to_owned()),
            operator_pin: None,
        };
        let second = ProofEnvelopeInput {
            subject_account_id: "account-replay-key-b".to_owned(),
            ..first.clone()
        };

        assert_ne!(
            replay_key("server-secret-a", &first),
            replay_key("server-secret-a", &second)
        );
    }

    #[tokio::test]
    async fn persisted_replay_material_is_not_plain_digest_over_low_entropy_secrets() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-hmac-persistence", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let code = display_code(
            &state,
            "venue-hmac-persistence",
            challenge.venue_key_version,
            received_at,
        )
        .await;
        let envelope: ProofEnvelopeInput = valid_envelope(&challenge, code.clone())
            .device_session_id("device-replay")
            .into();
        let plain_display_code_digest = digest_parts(&["display-code", code.trim()]);
        let plain_device_session_digest = digest_parts(&["device-session", "device-replay"]);
        let plain_replay_digest = digest_parts(&[
            "proof-envelope",
            envelope.subject_account_id.as_str(),
            envelope.challenge_id.as_deref().unwrap_or_default(),
            envelope.venue_id.as_deref().unwrap_or_default(),
            code.as_str(),
            &challenge.venue_key_version.to_string(),
            envelope.client_nonce.as_deref().unwrap_or_default(),
            &envelope
                .observed_at_ms
                .map(|value| value.to_string())
                .unwrap_or_default(),
            envelope
                .coarse_location_bucket
                .as_deref()
                .unwrap_or_default(),
            envelope.device_session_id.as_deref().unwrap_or_default(),
            envelope.fallback_mode.as_deref().unwrap_or_default(),
            envelope.operator_pin.as_deref().unwrap_or_default(),
        ]);

        let outcome = submit_proof_envelope_at(&state, envelope, received_at)
            .await
            .expect("valid proof should produce a result");

        assert!(outcome.accepted);
        let store = state.proof.read().await;
        let submission = store
            .proof_submissions_by_id
            .get(&outcome.proof_submission_id)
            .expect("submission should be retained");
        assert_ne!(submission.replay_key, plain_replay_digest);
        assert_ne!(
            submission.display_code_hash.as_deref(),
            Some(plain_display_code_digest.as_str())
        );
        assert_ne!(
            submission.device_session_id_hash.as_deref(),
            Some(plain_device_session_digest.as_str())
        );
        assert!(submission.raw_payload_json["display_code_hash"].is_string());
    }

    #[tokio::test]
    async fn previous_window_code_is_accepted_with_bounded_skew() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-prev-window", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let previous_code = display_code(
            &state,
            "venue-prev-window",
            challenge.venue_key_version,
            received_at - Duration::seconds(DISPLAY_CODE_WINDOW_SECONDS),
        )
        .await;

        let outcome = submit_proof_envelope_at(
            &state,
            valid_envelope(&challenge, previous_code).into(),
            received_at,
        )
        .await
        .expect("previous window proof should produce a result");

        assert!(outcome.accepted);
    }

    #[tokio::test]
    async fn subject_mismatch_submission_does_not_poison_valid_subject_submission() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-replay-subject", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let code = display_code(
            &state,
            "venue-replay-subject",
            challenge.venue_key_version,
            received_at,
        )
        .await;

        let mut mismatched: ProofEnvelopeInput = valid_envelope(&challenge, code.clone()).into();
        mismatched.subject_account_id = "other-account".to_owned();
        let mismatched_outcome = submit_proof_envelope_at(&state, mismatched, received_at)
            .await
            .expect("subject mismatch should still return a verification result");
        assert_eq!(
            mismatched_outcome.reason_code.as_deref(),
            Some(REASON_SUBJECT_MISMATCH)
        );

        let valid_outcome = submit_proof_envelope_at(
            &state,
            valid_envelope(&challenge, code).into(),
            received_at + Duration::seconds(1),
        )
        .await
        .expect("valid subject should not be blocked by another account");

        assert!(valid_outcome.accepted);
    }

    #[tokio::test]
    async fn public_venue_inputs_do_not_reconstruct_display_code() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-public-code", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let public_secret = digest_parts(&[
            "venue-secret",
            "realm-proof",
            "venue-public-code",
            &challenge.venue_key_version.to_string(),
        ]);
        let public_key = VenueKeyVersionRecord {
            realm_id: "realm-proof".to_owned(),
            venue_id: "venue-public-code".to_owned(),
            key_version: challenge.venue_key_version,
            secret_material: public_secret,
            status: KEY_STATUS_ACTIVE.to_owned(),
            not_before: received_at,
            not_after: None,
            created_at: received_at,
        };
        let mut forged_code = venue_display_code_for_window(&public_key, received_at);
        let real_code = display_code(
            &state,
            "venue-public-code",
            challenge.venue_key_version,
            received_at,
        )
        .await;
        if forged_code == real_code {
            forged_code = if real_code == "000000" {
                "000001".to_owned()
            } else {
                "000000".to_owned()
            };
        }

        let forged = submit_proof_envelope_at(
            &state,
            valid_envelope(&challenge, forged_code).into(),
            received_at,
        )
        .await
        .expect("forged proof should produce a result");

        assert!(!forged.accepted);
        assert_eq!(forged.reason_code.as_deref(), Some(REASON_INVALID_CODE));
    }

    #[test]
    fn venue_display_code_depends_on_server_secret() {
        let now = Utc::now();
        let mut first = ProofState::with_server_secret_for_test("proof-master-secret-a");
        let mut second = ProofState::with_server_secret_for_test("proof-master-secret-b");
        let key_version =
            ensure_active_venue_key(&mut first, "realm-proof", "venue-secret-test", now);
        let second_key_version =
            ensure_active_venue_key(&mut second, "realm-proof", "venue-secret-test", now);
        assert_eq!(key_version, second_key_version);

        let first_code = venue_display_code_for_window(
            first
                .venue_key_versions
                .get(&(
                    "realm-proof".to_owned(),
                    "venue-secret-test".to_owned(),
                    key_version,
                ))
                .expect("first key should exist"),
            now,
        );
        let second_code = venue_display_code_for_window(
            second
                .venue_key_versions
                .get(&(
                    "realm-proof".to_owned(),
                    "venue-secret-test".to_owned(),
                    second_key_version,
                ))
                .expect("second key should exist"),
            now,
        );

        assert_ne!(first_code, second_code);
    }

    #[tokio::test]
    async fn same_venue_id_in_different_realms_gets_different_display_codes() {
        let state = crate::new_state();
        let realm_a =
            start_test_challenge_in_realm(&state, "realm-a", "venue-shared", FALLBACK_NONE).await;
        let realm_b =
            start_test_challenge_in_realm(&state, "realm-b", "venue-shared", FALLBACK_NONE).await;
        let received_at = Utc::now();

        let realm_a_code = display_code_for_realm(
            &state,
            "realm-a",
            "venue-shared",
            realm_a.venue_key_version,
            received_at,
        )
        .await;
        let realm_b_code = display_code_for_realm(
            &state,
            "realm-b",
            "venue-shared",
            realm_b.venue_key_version,
            received_at,
        )
        .await;

        assert_ne!(realm_a_code, realm_b_code);

        let realm_a_outcome = submit_proof_envelope_at(
            &state,
            valid_envelope(&realm_a, realm_a_code).into(),
            received_at,
        )
        .await
        .expect("realm A proof should verify");
        let realm_b_outcome = submit_proof_envelope_at(
            &state,
            valid_envelope(&realm_b, realm_b_code).into(),
            received_at,
        )
        .await
        .expect("realm B proof should verify");

        assert!(realm_a_outcome.accepted);
        assert!(realm_b_outcome.accepted);
    }

    #[tokio::test]
    async fn challenge_does_not_accept_display_code_from_another_realm() {
        let state = crate::new_state();
        let realm_a =
            start_test_challenge_in_realm(&state, "realm-a", "venue-shared-proof", FALLBACK_NONE)
                .await;
        let realm_b =
            start_test_challenge_in_realm(&state, "realm-b", "venue-shared-proof", FALLBACK_NONE)
                .await;
        let received_at = Utc::now();
        let realm_b_code = display_code_for_realm(
            &state,
            "realm-b",
            "venue-shared-proof",
            realm_b.venue_key_version,
            received_at,
        )
        .await;

        let outcome = submit_proof_envelope_at(
            &state,
            valid_envelope(&realm_a, realm_b_code).into(),
            received_at,
        )
        .await
        .expect("cross-realm code proof should produce a result");

        assert!(!outcome.accepted);
        assert_eq!(outcome.reason_code.as_deref(), Some(REASON_INVALID_CODE));
    }

    #[tokio::test]
    async fn failed_attempt_limit_quarantines_challenge() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-attempt-limit", FALLBACK_NONE).await;
        let received_at = Utc::now();

        for attempt in 0..MAX_CHALLENGE_FAILED_ATTEMPTS {
            let mut envelope: ProofEnvelopeInput =
                valid_envelope(&challenge, impossible_display_code(attempt.into())).into();
            envelope.observed_at_ms =
                Some((received_at + Duration::seconds(attempt.into())).timestamp_millis());
            let outcome = submit_proof_envelope_at(
                &state,
                envelope,
                received_at + Duration::seconds(attempt.into()),
            )
            .await
            .expect("failed attempt should produce a result");

            if attempt + 1 < MAX_CHALLENGE_FAILED_ATTEMPTS {
                assert_eq!(outcome.verification_status, PROOF_STATUS_REJECTED);
                assert_eq!(outcome.reason_code.as_deref(), Some(REASON_INVALID_CODE));
            } else {
                assert_eq!(outcome.verification_status, PROOF_STATUS_QUARANTINED);
                assert_eq!(
                    outcome.reason_code.as_deref(),
                    Some(REASON_ATTEMPT_LIMIT_EXCEEDED)
                );
            }
        }

        let valid_code = display_code(
            &state,
            "venue-attempt-limit",
            challenge.venue_key_version,
            received_at,
        )
        .await;
        let valid_after_limit = submit_proof_envelope_at(
            &state,
            valid_envelope(&challenge, valid_code).into(),
            received_at,
        )
        .await
        .expect("proof after attempt limit should produce a result");

        assert!(!valid_after_limit.accepted);
        assert_eq!(
            valid_after_limit.reason_code.as_deref(),
            Some(REASON_ATTEMPT_LIMIT_EXCEEDED)
        );
    }

    #[tokio::test]
    async fn malformed_or_mismatched_requests_do_not_consume_attempt_budget() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-malformed-budget", FALLBACK_NONE).await;
        let received_at = Utc::now();

        for index in 0..5 {
            let outcome = submit_proof_envelope_at(
                &state,
                ProofEnvelopeInput {
                    subject_account_id: "account-proof-a".to_owned(),
                    challenge_id: Some(challenge.challenge_id.clone()),
                    venue_id: None,
                    display_code: Some(impossible_display_code(index as usize)),
                    key_version: Some(challenge.venue_key_version),
                    client_nonce: None,
                    observed_at_ms: Some(
                        (received_at + Duration::seconds(index)).timestamp_millis(),
                    ),
                    coarse_location_bucket: None,
                    device_session_id: Some(format!("malformed-device-{index}")),
                    fallback_mode: Some(FALLBACK_NONE.to_owned()),
                    operator_pin: None,
                },
                received_at + Duration::seconds(index),
            )
            .await
            .expect("malformed proof should produce a result");
            assert_eq!(outcome.reason_code.as_deref(), Some(REASON_MALFORMED));
        }

        for index in 0..5 {
            let mut envelope: ProofEnvelopeInput =
                valid_envelope(&challenge, format!("BAD-SUBJECT-{index}")).into();
            envelope.subject_account_id = format!("other-account-{index}");
            envelope.observed_at_ms =
                Some((received_at + Duration::seconds(10 + index)).timestamp_millis());
            let outcome = submit_proof_envelope_at(
                &state,
                envelope,
                received_at + Duration::seconds(10 + index),
            )
            .await
            .expect("subject mismatch proof should produce a result");
            assert_eq!(
                outcome.reason_code.as_deref(),
                Some(REASON_SUBJECT_MISMATCH)
            );
        }

        {
            let store = state.proof.read().await;
            let stored_challenge = store
                .venue_challenges_by_id
                .get(&challenge.challenge_id)
                .expect("challenge should remain recorded");
            assert_eq!(stored_challenge.failed_attempt_count, 0);
            assert_eq!(stored_challenge.status, CHALLENGE_STATUS_ISSUED);
        }

        let valid_code = display_code(
            &state,
            "venue-malformed-budget",
            challenge.venue_key_version,
            received_at,
        )
        .await;
        let valid_after_malformed = submit_proof_envelope_at(
            &state,
            valid_envelope(&challenge, valid_code).into(),
            received_at,
        )
        .await
        .expect("valid proof after malformed traffic should produce a result");

        assert!(valid_after_malformed.accepted);
    }

    #[tokio::test]
    async fn only_bound_secret_check_failures_consume_attempt_budget() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-secret-budget", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let mut missing_display_code: ProofEnvelopeInput =
            valid_envelope(&challenge, impossible_display_code(0)).into();
        missing_display_code.display_code = None;

        let missing = submit_proof_envelope_at(&state, missing_display_code, received_at)
            .await
            .expect("missing display code should produce a result");
        assert_eq!(missing.reason_code.as_deref(), Some(REASON_MALFORMED));

        let wrong_display_code = submit_proof_envelope_at(
            &state,
            valid_envelope(&challenge, impossible_display_code(1)).into(),
            received_at + Duration::seconds(1),
        )
        .await
        .expect("wrong display code should produce a result");
        assert_eq!(
            wrong_display_code.reason_code.as_deref(),
            Some(REASON_INVALID_CODE)
        );

        {
            let store = state.proof.read().await;
            let stored_challenge = store
                .venue_challenges_by_id
                .get(&challenge.challenge_id)
                .expect("challenge should remain recorded");
            assert_eq!(stored_challenge.failed_attempt_count, 1);
        }

        let operator_start = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-operator-budget".to_owned(),
                venue_id: "venue-operator-secret-budget".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-secret-budget".to_owned()),
            },
        )
        .await
        .expect("operator fallback challenge should issue");
        let operator_challenge = operator_start.client;
        let issued_pin = operator_start
            .operator_delivery
            .expect("operator delivery should exist")
            .pin;
        let invalid_pin = if issued_pin == "000000" {
            "000001"
        } else {
            "000000"
        };
        let wrong_pin = submit_proof_envelope_at(
            &state,
            ProofEnvelopeInput {
                subject_account_id: "account-operator-budget".to_owned(),
                challenge_id: Some(operator_challenge.challenge_id.clone()),
                venue_id: Some(operator_challenge.venue_id.clone()),
                display_code: None,
                key_version: None,
                client_nonce: Some(operator_challenge.client_nonce.clone()),
                observed_at_ms: Some(received_at.timestamp_millis()),
                coarse_location_bucket: None,
                device_session_id: None,
                fallback_mode: Some(FALLBACK_OPERATOR_PIN.to_owned()),
                operator_pin: Some(invalid_pin.to_owned()),
            },
            received_at,
        )
        .await
        .expect("wrong operator PIN should produce a result");

        assert_eq!(
            wrong_pin.reason_code.as_deref(),
            Some(REASON_OPERATOR_PIN_INVALID)
        );
        let store = state.proof.read().await;
        let stored_operator_challenge = store
            .venue_challenges_by_id
            .get(&operator_challenge.challenge_id)
            .expect("operator challenge should remain recorded");
        assert_eq!(stored_operator_challenge.failed_attempt_count, 1);
    }

    #[tokio::test]
    async fn attempt_limit_counts_only_real_secret_check_failures() {
        let state = crate::new_state();
        let challenge =
            start_test_challenge(&state, "venue-attempt-limit-secret-only", FALLBACK_NONE).await;
        let received_at = Utc::now();

        for index in 0..MAX_CHALLENGE_FAILED_ATTEMPTS {
            let malformed = ProofEnvelopeInput {
                subject_account_id: "account-proof-a".to_owned(),
                challenge_id: Some(challenge.challenge_id.clone()),
                venue_id: Some(challenge.venue_id.clone()),
                display_code: Some(format!("BAD-MALFORMED-{index}")),
                key_version: Some(challenge.venue_key_version),
                client_nonce: None,
                observed_at_ms: Some(
                    (received_at + Duration::seconds(index.into())).timestamp_millis(),
                ),
                coarse_location_bucket: None,
                device_session_id: None,
                fallback_mode: Some(FALLBACK_NONE.to_owned()),
                operator_pin: None,
            };
            let outcome = submit_proof_envelope_at(
                &state,
                malformed,
                received_at + Duration::seconds(index.into()),
            )
            .await
            .expect("malformed proof should produce a result");
            assert_eq!(outcome.reason_code.as_deref(), Some(REASON_MALFORMED));
        }

        for attempt in 0..MAX_CHALLENGE_FAILED_ATTEMPTS {
            let mut envelope: ProofEnvelopeInput =
                valid_envelope(&challenge, impossible_display_code(attempt.into())).into();
            envelope.observed_at_ms =
                Some((received_at + Duration::seconds(10 + i64::from(attempt))).timestamp_millis());
            let outcome = submit_proof_envelope_at(
                &state,
                envelope,
                received_at + Duration::seconds(10 + i64::from(attempt)),
            )
            .await
            .expect("secret-check failure should produce a result");

            if attempt + 1 < MAX_CHALLENGE_FAILED_ATTEMPTS {
                assert_eq!(outcome.reason_code.as_deref(), Some(REASON_INVALID_CODE));
            } else {
                assert_eq!(
                    outcome.reason_code.as_deref(),
                    Some(REASON_ATTEMPT_LIMIT_EXCEEDED)
                );
            }
        }
    }

    #[tokio::test]
    async fn stale_client_observation_is_quarantined() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-stale", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let code = display_code(
            &state,
            "venue-stale",
            challenge.venue_key_version,
            received_at,
        )
        .await;
        let mut envelope: ProofEnvelopeInput = valid_envelope(&challenge, code).into();
        envelope.observed_at_ms = Some((received_at - Duration::seconds(121)).timestamp_millis());

        let outcome = submit_proof_envelope_at(&state, envelope, received_at)
            .await
            .expect("stale proof should produce a result");

        assert!(!outcome.accepted);
        assert_eq!(outcome.verification_status, PROOF_STATUS_QUARANTINED);
        assert_eq!(outcome.reason_code.as_deref(), Some(REASON_RISK_FLAGGED));
        assert!(
            outcome
                .risk_flags
                .iter()
                .any(|flag| flag == "client_clock_skew_high")
        );
    }

    #[tokio::test]
    async fn revoked_key_version_is_rejected() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-revoked", FALLBACK_NONE).await;
        let code = display_code(
            &state,
            "venue-revoked",
            challenge.venue_key_version,
            Utc::now(),
        )
        .await;
        {
            let mut store = state.proof.write().await;
            let key = store
                .venue_key_versions
                .get_mut(&(
                    "realm-proof".to_owned(),
                    "venue-revoked".to_owned(),
                    challenge.venue_key_version,
                ))
                .expect("venue key should exist");
            key.status = KEY_STATUS_REVOKED.to_owned();
        }

        let outcome = submit_proof_envelope(&state, valid_envelope(&challenge, code).into())
            .await
            .expect("revoked key proof should produce a result");

        assert!(!outcome.accepted);
        assert_eq!(outcome.reason_code.as_deref(), Some(REASON_KEY_REVOKED));
    }

    #[tokio::test]
    async fn draining_key_version_accepts_existing_challenge_only() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-draining", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let code = display_code(
            &state,
            "venue-draining",
            challenge.venue_key_version,
            received_at,
        )
        .await;
        {
            let mut store = state.proof.write().await;
            let old_key = store
                .venue_key_versions
                .get_mut(&(
                    "realm-proof".to_owned(),
                    "venue-draining".to_owned(),
                    challenge.venue_key_version,
                ))
                .expect("old venue key should exist");
            old_key.status = KEY_STATUS_DRAINING.to_owned();
            let new_key_version = challenge.venue_key_version + 1;
            store.active_key_version_by_venue.insert(
                ("realm-proof".to_owned(), "venue-draining".to_owned()),
                new_key_version,
            );
            let secret_material = venue_secret_material(
                &store.server_secret,
                "realm-proof",
                "venue-draining",
                new_key_version,
            );
            store.venue_key_versions.insert(
                (
                    "realm-proof".to_owned(),
                    "venue-draining".to_owned(),
                    new_key_version,
                ),
                VenueKeyVersionRecord {
                    realm_id: "realm-proof".to_owned(),
                    venue_id: "venue-draining".to_owned(),
                    key_version: new_key_version,
                    secret_material,
                    status: KEY_STATUS_ACTIVE.to_owned(),
                    not_before: received_at,
                    not_after: None,
                    created_at: received_at,
                },
            );
        }

        let outcome =
            submit_proof_envelope_at(&state, valid_envelope(&challenge, code).into(), received_at)
                .await
                .expect("draining key proof should produce a result");

        assert!(outcome.accepted);
    }

    #[tokio::test]
    async fn old_challenge_rejects_submitted_key_version_override() {
        let state = crate::new_state();
        let challenge = start_test_challenge(&state, "venue-old-key-override", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let new_key_version = challenge.venue_key_version + 1;
        {
            let mut store = state.proof.write().await;
            store.active_key_version_by_venue.insert(
                (
                    "realm-proof".to_owned(),
                    "venue-old-key-override".to_owned(),
                ),
                new_key_version,
            );
            let secret_material = venue_secret_material(
                &store.server_secret,
                "realm-proof",
                "venue-old-key-override",
                new_key_version,
            );
            store.venue_key_versions.insert(
                (
                    "realm-proof".to_owned(),
                    "venue-old-key-override".to_owned(),
                    new_key_version,
                ),
                VenueKeyVersionRecord {
                    realm_id: "realm-proof".to_owned(),
                    venue_id: "venue-old-key-override".to_owned(),
                    key_version: new_key_version,
                    secret_material,
                    status: KEY_STATUS_ACTIVE.to_owned(),
                    not_before: received_at,
                    not_after: None,
                    created_at: received_at,
                },
            );
        }
        let new_code = display_code(
            &state,
            "venue-old-key-override",
            new_key_version,
            received_at,
        )
        .await;
        let mut envelope: ProofEnvelopeInput = valid_envelope(&challenge, new_code).into();
        envelope.key_version = Some(new_key_version);

        let outcome = submit_proof_envelope_at(&state, envelope, received_at)
            .await
            .expect("key-version override proof should produce a result");

        assert!(!outcome.accepted);
        assert_eq!(
            outcome.reason_code.as_deref(),
            Some(REASON_KEY_VERSION_MISMATCH)
        );
        let store = state.proof.read().await;
        let stored_challenge = store
            .venue_challenges_by_id
            .get(&challenge.challenge_id)
            .expect("challenge should remain recorded");
        assert_eq!(stored_challenge.failed_attempt_count, 0);
    }

    #[tokio::test]
    async fn new_challenge_rejects_old_draining_key_override() {
        let state = crate::new_state();
        let first = start_test_challenge(&state, "venue-new-key-override", FALLBACK_NONE).await;
        let received_at = Utc::now();
        let old_key_version = first.venue_key_version;
        let new_key_version = old_key_version + 1;
        {
            let mut store = state.proof.write().await;
            let old_key = store
                .venue_key_versions
                .get_mut(&(
                    "realm-proof".to_owned(),
                    "venue-new-key-override".to_owned(),
                    old_key_version,
                ))
                .expect("old key should exist");
            old_key.status = KEY_STATUS_DRAINING.to_owned();
            store.active_key_version_by_venue.insert(
                (
                    "realm-proof".to_owned(),
                    "venue-new-key-override".to_owned(),
                ),
                new_key_version,
            );
            let secret_material = venue_secret_material(
                &store.server_secret,
                "realm-proof",
                "venue-new-key-override",
                new_key_version,
            );
            store.venue_key_versions.insert(
                (
                    "realm-proof".to_owned(),
                    "venue-new-key-override".to_owned(),
                    new_key_version,
                ),
                VenueKeyVersionRecord {
                    realm_id: "realm-proof".to_owned(),
                    venue_id: "venue-new-key-override".to_owned(),
                    key_version: new_key_version,
                    secret_material,
                    status: KEY_STATUS_ACTIVE.to_owned(),
                    not_before: received_at,
                    not_after: None,
                    created_at: received_at,
                },
            );
        }
        let second = start_test_challenge(&state, "venue-new-key-override", FALLBACK_NONE).await;
        assert_eq!(second.venue_key_version, new_key_version);
        let old_code = display_code(
            &state,
            "venue-new-key-override",
            old_key_version,
            received_at,
        )
        .await;
        let mut envelope: ProofEnvelopeInput = valid_envelope(&second, old_code).into();
        envelope.key_version = Some(old_key_version);

        let outcome = submit_proof_envelope_at(&state, envelope, received_at)
            .await
            .expect("old draining key override proof should produce a result");

        assert!(!outcome.accepted);
        assert_eq!(
            outcome.reason_code.as_deref(),
            Some(REASON_KEY_VERSION_MISMATCH)
        );
    }

    #[tokio::test]
    async fn operator_pin_flow_is_audited_and_rate_limited() {
        let state = crate::new_state();
        let first_start = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-operator-0".to_owned(),
                venue_id: "venue-operator".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-a".to_owned()),
            },
        )
        .await
        .expect("operator pin within rate limit should issue");
        let first_challenge = first_start.client;
        let first_delivery = first_start
            .operator_delivery
            .expect("operator delivery should be separated from client outcome");
        assert!(first_challenge.operator_pin_issued);
        assert_eq!(first_delivery.challenge_id, first_challenge.challenge_id);

        let outcome = submit_proof_envelope(
            &state,
            ProofEnvelopeInput {
                subject_account_id: "account-operator-0".to_owned(),
                challenge_id: Some(first_challenge.challenge_id.clone()),
                venue_id: Some(first_challenge.venue_id.clone()),
                display_code: None,
                key_version: None,
                client_nonce: Some(first_challenge.client_nonce.clone()),
                observed_at_ms: Some(Utc::now().timestamp_millis()),
                coarse_location_bucket: None,
                device_session_id: None,
                fallback_mode: Some(FALLBACK_OPERATOR_PIN.to_owned()),
                operator_pin: Some(first_delivery.pin.clone()),
            },
        )
        .await
        .expect("operator pin fallback should produce a result");
        assert!(outcome.accepted);
        assert!(
            outcome
                .risk_flags
                .iter()
                .any(|flag| flag == "operator_fallback")
        );

        for index in 1..OPERATOR_PIN_RATE_LIMIT_PER_MINUTE {
            start_proof_challenge(
                &state,
                StartProofChallengeInput {
                    subject_account_id: format!("account-{index}"),
                    venue_id: "venue-operator".to_owned(),
                    realm_id: "realm-proof".to_owned(),
                    fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                    operator_id: Some("operator-a".to_owned()),
                },
            )
            .await
            .expect("operator pin within rate limit should issue");
        }

        let blocked = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-blocked".to_owned(),
                venue_id: "venue-operator".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-a".to_owned()),
            },
        )
        .await
        .expect_err("operator pin overuse should be rate-limited");
        assert_eq!(
            blocked.message(),
            "operator_pin fallback rate limit exceeded"
        );

        let store = state.proof.read().await;
        assert_eq!(
            store.operator_pin_audits.len(),
            OPERATOR_PIN_RATE_LIMIT_PER_MINUTE
        );
        let first_audit = store
            .operator_pin_audits
            .iter()
            .find(|audit| audit.challenge_id == first_challenge.challenge_id)
            .expect("first operator PIN audit should exist");
        assert_ne!(
            hex_bytes(&first_audit.pin_hash),
            digest_parts(&[
                "operator-pin",
                &first_challenge.challenge_id,
                &first_delivery.pin
            ])
        );
    }

    #[tokio::test]
    async fn operator_pin_capable_challenge_accepts_normal_display_code_flow() {
        let state = crate::new_state();
        let start = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-proof-a".to_owned(),
                venue_id: "venue-operator-display-code".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-display-code".to_owned()),
            },
        )
        .await
        .expect("operator fallback challenge should issue");
        let challenge = start.client;
        let code = display_code_for_realm(
            &state,
            &challenge.realm_id,
            &challenge.venue_id,
            challenge.venue_key_version,
            Utc::now(),
        )
        .await;

        let outcome = submit_proof_envelope(&state, valid_envelope(&challenge, code).into())
            .await
            .expect("normal display-code flow should remain valid");

        assert!(outcome.accepted);
        assert!(
            !outcome
                .risk_flags
                .iter()
                .any(|flag| flag == "operator_fallback")
        );
    }

    #[tokio::test]
    async fn public_operator_inputs_do_not_reconstruct_pin() {
        let state = crate::new_state();
        let start = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-public-pin".to_owned(),
                venue_id: "venue-public-pin".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-public".to_owned()),
            },
        )
        .await
        .expect("operator pin should issue");
        let challenge = start.client;
        let delivery = start
            .operator_delivery
            .expect("operator delivery should exist");
        let old_pin_seed = format!(
            "operator-pin:{}:{}",
            challenge.challenge_id, delivery.operator_id
        );
        let mut old_publicly_derivable_pin =
            short_code_from_bytes(&Sha256::digest(old_pin_seed.as_bytes()));
        if old_publicly_derivable_pin == delivery.pin {
            old_publicly_derivable_pin = if delivery.pin == "000000" {
                "000001".to_owned()
            } else {
                "000000".to_owned()
            };
        }

        let forged = submit_proof_envelope(
            &state,
            ProofEnvelopeInput {
                subject_account_id: "account-public-pin".to_owned(),
                challenge_id: Some(challenge.challenge_id.clone()),
                venue_id: Some(challenge.venue_id.clone()),
                display_code: None,
                key_version: None,
                client_nonce: Some(challenge.client_nonce.clone()),
                observed_at_ms: Some(Utc::now().timestamp_millis()),
                coarse_location_bucket: None,
                device_session_id: None,
                fallback_mode: Some(FALLBACK_OPERATOR_PIN.to_owned()),
                operator_pin: Some(old_publicly_derivable_pin),
            },
        )
        .await
        .expect("forged operator pin should produce a result");

        assert!(!forged.accepted);
        assert_eq!(
            forged.reason_code.as_deref(),
            Some(REASON_OPERATOR_PIN_INVALID)
        );
    }

    #[tokio::test]
    async fn same_operator_and_venue_get_distinct_pins_per_challenge() {
        let state = crate::new_state();
        let first = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-random-pin-a".to_owned(),
                venue_id: "venue-random-pin".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-random".to_owned()),
            },
        )
        .await
        .expect("first operator pin should issue");
        let second = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-random-pin-b".to_owned(),
                venue_id: "venue-random-pin".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-random".to_owned()),
            },
        )
        .await
        .expect("second operator pin should issue");

        let first_pin = first.operator_delivery.expect("first delivery").pin;
        let second_pin = second.operator_delivery.expect("second delivery").pin;
        assert_ne!(first_pin, second_pin);
    }

    #[tokio::test]
    async fn malformed_envelope_is_rejected_and_recorded() {
        let state = crate::new_state();

        let outcome = submit_proof_envelope(
            &state,
            ProofEnvelopeInput {
                subject_account_id: "account-malformed".to_owned(),
                challenge_id: None,
                venue_id: Some("venue-malformed".to_owned()),
                display_code: None,
                key_version: None,
                client_nonce: None,
                observed_at_ms: None,
                coarse_location_bucket: None,
                device_session_id: Some("device-1".to_owned()),
                fallback_mode: None,
                operator_pin: None,
            },
        )
        .await
        .expect("malformed proof should produce a verification result");

        assert!(!outcome.accepted);
        assert_eq!(outcome.reason_code.as_deref(), Some(REASON_MALFORMED));
        let store = state.proof.read().await;
        assert!(
            store
                .proof_submissions_by_id
                .contains_key(&outcome.proof_submission_id)
        );
    }

    #[tokio::test]
    async fn redacted_payload_canonicalizes_request_identifiers() {
        let state = crate::new_state();
        let outcome = submit_proof_envelope(
            &state,
            ProofEnvelopeInput {
                subject_account_id: "account-redacted".to_owned(),
                challenge_id: Some("  challenge-redacted  ".to_owned()),
                venue_id: Some("  venue-redacted  ".to_owned()),
                display_code: None,
                key_version: None,
                client_nonce: None,
                observed_at_ms: None,
                coarse_location_bucket: None,
                device_session_id: None,
                fallback_mode: None,
                operator_pin: None,
            },
        )
        .await
        .expect("malformed proof should still be recorded");

        let store = state.proof.read().await;
        let submission = store
            .proof_submissions_by_id
            .get(&outcome.proof_submission_id)
            .expect("submission should be retained");
        assert_eq!(
            submission.raw_payload_json["challenge_id"].as_str(),
            Some("challenge-redacted")
        );
        assert_eq!(
            submission.raw_payload_json["venue_id"].as_str(),
            Some("venue-redacted")
        );
    }

    #[tokio::test]
    async fn proof_submission_state_prunes_expired_rejected_evidence() {
        let state = crate::new_state();
        let base = Utc::now();
        let old_input = ProofEnvelopeInput {
            subject_account_id: "account-prune-old".to_owned(),
            challenge_id: Some("challenge-prune-old".to_owned()),
            venue_id: Some("venue-prune".to_owned()),
            display_code: None,
            key_version: None,
            client_nonce: None,
            observed_at_ms: None,
            coarse_location_bucket: None,
            device_session_id: None,
            fallback_mode: None,
            operator_pin: None,
        };
        let old_outcome = submit_proof_envelope_at(&state, old_input.clone(), base)
            .await
            .expect("old malformed proof should be recorded");

        let new_input = ProofEnvelopeInput {
            subject_account_id: "account-prune-new".to_owned(),
            challenge_id: Some("challenge-prune-new".to_owned()),
            venue_id: Some("venue-prune".to_owned()),
            display_code: None,
            key_version: None,
            client_nonce: None,
            observed_at_ms: None,
            coarse_location_bucket: None,
            device_session_id: None,
            fallback_mode: None,
            operator_pin: None,
        };
        let new_outcome = submit_proof_envelope_at(
            &state,
            new_input.clone(),
            base + Duration::seconds(PROOF_SUBMISSION_RETENTION_SECONDS + 1),
        )
        .await
        .expect("new malformed proof should be recorded");

        let store = state.proof.read().await;
        assert!(
            !store
                .proof_submissions_by_id
                .contains_key(&old_outcome.proof_submission_id)
        );
        assert!(
            store
                .proof_submissions_by_id
                .contains_key(&new_outcome.proof_submission_id)
        );
        assert_eq!(
            store
                .proof_submission_id_by_replay_key
                .get(&replay_key(&store.server_secret, &old_input)),
            None
        );
        assert!(
            store
                .proof_verifications_by_id
                .values()
                .all(|verification| verification.proof_submission_id
                    != old_outcome.proof_submission_id)
        );
    }

    #[tokio::test]
    async fn replay_index_tracks_latest_retained_duplicate_submission() {
        let state = crate::new_state();
        let base = Utc::now();
        let input = ProofEnvelopeInput {
            subject_account_id: "account-replay-prune".to_owned(),
            challenge_id: Some("challenge-replay-prune".to_owned()),
            venue_id: Some("venue-replay-prune".to_owned()),
            display_code: None,
            key_version: None,
            client_nonce: None,
            observed_at_ms: None,
            coarse_location_bucket: None,
            device_session_id: None,
            fallback_mode: None,
            operator_pin: None,
        };

        let first = submit_proof_envelope_at(&state, input.clone(), base)
            .await
            .expect("first malformed proof should be recorded");
        let second = submit_proof_envelope_at(&state, input.clone(), base + Duration::seconds(2))
            .await
            .expect("duplicate malformed proof should be recorded as replay");
        assert_eq!(second.reason_code.as_deref(), Some(REASON_REPLAY));

        let third = submit_proof_envelope_at(
            &state,
            input.clone(),
            base + Duration::seconds(PROOF_SUBMISSION_RETENTION_SECONDS + 1),
        )
        .await
        .expect("retained duplicate should still be treated as replay");
        assert_eq!(third.reason_code.as_deref(), Some(REASON_REPLAY));

        let store = state.proof.read().await;
        let replay_key = replay_key(&store.server_secret, &input);
        assert_eq!(
            store.proof_submission_id_by_replay_key.get(&replay_key),
            Some(&third.proof_submission_id)
        );
        assert!(
            !store
                .proof_submissions_by_id
                .contains_key(&first.proof_submission_id)
        );
    }

    #[tokio::test]
    async fn proof_submission_state_caps_unique_rejected_evidence() {
        let state = crate::new_state();
        let base = Utc::now();
        let mut first_submission_id = None;
        let mut last_submission_id = None;

        for index in 0..=MAX_RETAINED_PROOF_SUBMISSIONS {
            let outcome = submit_proof_envelope_at(
                &state,
                ProofEnvelopeInput {
                    subject_account_id: format!("account-cap-{index}"),
                    challenge_id: Some(format!("challenge-cap-{index}")),
                    venue_id: Some("venue-cap".to_owned()),
                    display_code: None,
                    key_version: None,
                    client_nonce: None,
                    observed_at_ms: None,
                    coarse_location_bucket: None,
                    device_session_id: None,
                    fallback_mode: None,
                    operator_pin: None,
                },
                base + Duration::milliseconds(index as i64),
            )
            .await
            .expect("malformed proof should be recorded");

            if index == 0 {
                first_submission_id = Some(outcome.proof_submission_id.clone());
            }
            if index == MAX_RETAINED_PROOF_SUBMISSIONS {
                last_submission_id = Some(outcome.proof_submission_id.clone());
            }
        }

        let store = state.proof.read().await;
        assert_eq!(
            store.proof_submissions_by_id.len(),
            MAX_RETAINED_PROOF_SUBMISSIONS
        );
        assert_eq!(
            store.proof_verifications_by_id.len(),
            MAX_RETAINED_PROOF_SUBMISSIONS
        );
        assert!(
            !store
                .proof_submissions_by_id
                .contains_key(&first_submission_id.expect("first submission id"))
        );
        assert!(
            store
                .proof_submissions_by_id
                .contains_key(&last_submission_id.expect("last submission id"))
        );
    }

    #[tokio::test]
    async fn challenge_state_prunes_expired_challenges_and_stale_operator_audits() {
        let state = crate::new_state();
        let first = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-prune-challenge-0".to_owned(),
                venue_id: "venue-prune-challenge".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_OPERATOR_PIN.to_owned(),
                operator_id: Some("operator-prune".to_owned()),
            },
        )
        .await
        .expect("operator fallback challenge should issue")
        .client;

        {
            let mut store = state.proof.write().await;
            let challenge = store
                .venue_challenges_by_id
                .get_mut(&first.challenge_id)
                .expect("first challenge should be retained before pruning");
            let stale_at = Utc::now() - Duration::seconds(PROOF_CHALLENGE_RETENTION_SECONDS + 1);
            challenge.expires_at = stale_at;
            challenge.operator_pin_expires_at = Some(stale_at);

            let audit = store
                .operator_pin_audits
                .iter_mut()
                .find(|audit| audit.challenge_id == first.challenge_id)
                .expect("operator audit should exist");
            let stale_audit_at =
                Utc::now() - Duration::seconds(OPERATOR_PIN_RATE_LIMIT_WINDOW_SECONDS + 1);
            audit.issued_at = stale_audit_at;
            audit.expires_at = stale_audit_at;
        }

        let second = start_test_challenge(&state, "venue-prune-challenge", FALLBACK_NONE).await;

        let store = state.proof.read().await;
        assert!(
            !store
                .venue_challenges_by_id
                .contains_key(&first.challenge_id)
        );
        assert!(
            store
                .venue_challenges_by_id
                .contains_key(&second.challenge_id)
        );
        assert!(store.operator_pin_audits.is_empty());
    }

    #[tokio::test]
    async fn active_challenge_limit_is_enforced_per_subject() {
        let state = crate::new_state();

        for index in 0..MAX_ACTIVE_PROOF_CHALLENGES_PER_SUBJECT {
            start_proof_challenge(
                &state,
                StartProofChallengeInput {
                    subject_account_id: "account-challenge-cap".to_owned(),
                    venue_id: format!("venue-challenge-cap-{index}"),
                    realm_id: "realm-proof".to_owned(),
                    fallback_mode: FALLBACK_NONE.to_owned(),
                    operator_id: None,
                },
            )
            .await
            .expect("challenge within subject cap should issue");
        }

        let blocked = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-challenge-cap".to_owned(),
                venue_id: "venue-challenge-cap-blocked".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_NONE.to_owned(),
                operator_id: None,
            },
        )
        .await
        .expect_err("subject beyond active challenge cap should be rejected");
        assert_eq!(blocked.message(), "active proof challenge limit exceeded");
    }

    #[tokio::test]
    async fn active_challenge_capacity_preserves_existing_active_challenges() {
        let state = crate::new_state();
        let first = start_test_challenge(&state, "venue-challenge-cap-first", FALLBACK_NONE).await;

        {
            let now = Utc::now();
            let mut store = state.proof.write().await;
            for index in 0..MAX_ACTIVE_PROOF_CHALLENGES - 1 {
                let challenge_id = format!("manual-active-challenge-{index}");
                let venue_id = format!("manual-active-venue-{index}");
                store.venue_challenges_by_id.insert(
                    challenge_id.clone(),
                    VenueChallengeRecord {
                        challenge_id: challenge_id.clone(),
                        subject_account_id: format!("manual-subject-{index}"),
                        venue_id,
                        realm_id: "realm-proof".to_owned(),
                        client_nonce_hash: digest_parts(&["client-nonce", &challenge_id, "manual"]),
                        issued_at: now,
                        expires_at: now + Duration::seconds(CHALLENGE_TTL_SECONDS),
                        consumed_at: None,
                        fallback_mode: FALLBACK_NONE.to_owned(),
                        operator_pin_hash: None,
                        operator_pin_expires_at: None,
                        operator_id: None,
                        venue_key_version: 1,
                        failed_attempt_count: 0,
                        status: CHALLENGE_STATUS_ISSUED.to_owned(),
                    },
                );
            }
        }

        let blocked = start_proof_challenge(
            &state,
            StartProofChallengeInput {
                subject_account_id: "account-capacity-blocked".to_owned(),
                venue_id: "venue-capacity-blocked".to_owned(),
                realm_id: "realm-proof".to_owned(),
                fallback_mode: FALLBACK_NONE.to_owned(),
                operator_id: None,
            },
        )
        .await
        .expect_err("global active challenge capacity should reject new issuance");
        assert_eq!(blocked.message(), "proof challenge capacity exceeded");

        let store = state.proof.read().await;
        assert_eq!(
            active_challenge_count(&store, Utc::now()),
            MAX_ACTIVE_PROOF_CHALLENGES
        );
        assert!(
            store
                .venue_challenges_by_id
                .contains_key(&first.challenge_id)
        );
    }

    #[tokio::test]
    async fn challenge_pruning_removes_unused_venue_key_state() {
        let state = crate::new_state();
        let first = start_test_challenge(&state, "venue-key-prune", FALLBACK_NONE).await;

        {
            let mut store = state.proof.write().await;
            let stale_at = Utc::now() - Duration::seconds(PROOF_CHALLENGE_RETENTION_SECONDS + 1);
            let challenge = store
                .venue_challenges_by_id
                .get_mut(&first.challenge_id)
                .expect("challenge should exist before pruning");
            challenge.expires_at = stale_at;
        }

        let second = start_test_challenge(&state, "venue-key-prune-next", FALLBACK_NONE).await;

        let store = state.proof.read().await;
        assert!(
            !store
                .venue_challenges_by_id
                .contains_key(&first.challenge_id)
        );
        assert!(
            !store
                .active_key_version_by_venue
                .contains_key(&("realm-proof".to_owned(), "venue-key-prune".to_owned()))
        );
        assert!(!store.venue_key_versions.contains_key(&(
            "realm-proof".to_owned(),
            "venue-key-prune".to_owned(),
            first.venue_key_version
        )));
        assert!(
            store
                .active_key_version_by_venue
                .contains_key(&("realm-proof".to_owned(), second.venue_id.clone()))
        );
    }

    #[tokio::test]
    async fn retained_challenge_keeps_active_rotated_venue_key_state() {
        let state = crate::new_state();
        let first = start_test_challenge(&state, "venue-key-retain", FALLBACK_NONE).await;
        let rotated_key_version = first.venue_key_version + 1;
        let now = Utc::now();

        {
            let mut store = state.proof.write().await;
            store.active_key_version_by_venue.insert(
                ("realm-proof".to_owned(), "venue-key-retain".to_owned()),
                rotated_key_version,
            );
            let secret_material = venue_secret_material(
                &store.server_secret,
                "realm-proof",
                "venue-key-retain",
                rotated_key_version,
            );
            store.venue_key_versions.insert(
                (
                    "realm-proof".to_owned(),
                    "venue-key-retain".to_owned(),
                    rotated_key_version,
                ),
                VenueKeyVersionRecord {
                    realm_id: "realm-proof".to_owned(),
                    venue_id: "venue-key-retain".to_owned(),
                    key_version: rotated_key_version,
                    secret_material,
                    status: KEY_STATUS_ACTIVE.to_owned(),
                    not_before: now,
                    not_after: None,
                    created_at: now,
                },
            );
            prune_ephemeral_proof_state(&mut store, now);
        }

        let second = start_test_challenge(&state, "venue-key-retain", FALLBACK_NONE).await;
        assert_eq!(second.venue_key_version, rotated_key_version);
    }

    fn impossible_display_code(index: usize) -> String {
        format!("Z{index:05}")
    }

    async fn start_test_challenge(
        state: &SharedState,
        venue_id: &str,
        fallback_mode: &str,
    ) -> StartProofChallengeOutcome {
        start_test_challenge_in_realm(state, "realm-proof", venue_id, fallback_mode).await
    }

    async fn start_test_challenge_in_realm(
        state: &SharedState,
        realm_id: &str,
        venue_id: &str,
        fallback_mode: &str,
    ) -> StartProofChallengeOutcome {
        start_proof_challenge(
            state,
            StartProofChallengeInput {
                subject_account_id: "account-proof-a".to_owned(),
                venue_id: venue_id.to_owned(),
                realm_id: realm_id.to_owned(),
                fallback_mode: fallback_mode.to_owned(),
                operator_id: if fallback_mode == FALLBACK_OPERATOR_PIN {
                    Some("operator-a".to_owned())
                } else {
                    None
                },
            },
        )
        .await
        .expect("challenge should issue")
        .client
    }

    async fn display_code(
        state: &SharedState,
        venue_id: &str,
        key_version: i32,
        at: DateTime<Utc>,
    ) -> String {
        display_code_for_realm(state, "realm-proof", venue_id, key_version, at).await
    }

    async fn display_code_for_realm(
        state: &SharedState,
        realm_id: &str,
        venue_id: &str,
        key_version: i32,
        at: DateTime<Utc>,
    ) -> String {
        let store = state.proof.read().await;
        let key = store
            .venue_key_versions
            .get(&(realm_id.to_owned(), venue_id.to_owned(), key_version))
            .expect("venue key should exist");
        venue_display_code_for_window(key, at)
    }

    fn valid_envelope(challenge: &StartProofChallengeOutcome, code: String) -> TestEnvelopeBuilder {
        TestEnvelopeBuilder(ProofEnvelopeInput {
            subject_account_id: "account-proof-a".to_owned(),
            challenge_id: Some(challenge.challenge_id.clone()),
            venue_id: Some(challenge.venue_id.clone()),
            display_code: Some(code),
            key_version: Some(challenge.venue_key_version),
            client_nonce: Some(challenge.client_nonce.clone()),
            observed_at_ms: Some(Utc::now().timestamp_millis()),
            coarse_location_bucket: None,
            device_session_id: None,
            fallback_mode: Some(FALLBACK_NONE.to_owned()),
            operator_pin: None,
        })
    }

    #[derive(Clone)]
    struct TestEnvelopeBuilder(ProofEnvelopeInput);

    impl TestEnvelopeBuilder {
        fn coarse_location_bucket(mut self, value: &str) -> Self {
            self.0.coarse_location_bucket = Some(value.to_owned());
            self
        }

        fn device_session_id(mut self, value: &str) -> Self {
            self.0.device_session_id = Some(value.to_owned());
            self
        }

        fn fallback_mode(mut self, value: &str) -> Self {
            self.0.fallback_mode = Some(value.to_owned());
            self
        }
    }

    impl From<TestEnvelopeBuilder> for ProofEnvelopeInput {
        fn from(value: TestEnvelopeBuilder) -> Self {
            value.0
        }
    }
}
