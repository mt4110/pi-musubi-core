use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

const MODE_KEY: &str = "MUSUBI_LAUNCH_MODE";
const ALLOWLIST_PI_UIDS_KEY: &str = "MUSUBI_LAUNCH_ALLOWLIST_PI_UIDS";
const ALLOWLIST_ACCOUNT_IDS_KEY: &str = "MUSUBI_LAUNCH_ALLOWLIST_ACCOUNT_IDS";
const SUPPORT_CONTACT_URL_KEY: &str = "MUSUBI_LAUNCH_SUPPORT_CONTACT_URL";
const SUPPORT_CONTACT_LABEL_KEY: &str = "MUSUBI_LAUNCH_SUPPORT_CONTACT_LABEL";
const KILL_SWITCH_AUTH_KEY: &str = "MUSUBI_KILL_SWITCH_AUTH";
const KILL_SWITCH_PROMISE_CREATION_KEY: &str = "MUSUBI_KILL_SWITCH_PROMISE_CREATION";
const KILL_SWITCH_PROOF_CHALLENGE_KEY: &str = "MUSUBI_KILL_SWITCH_PROOF_CHALLENGE";
const KILL_SWITCH_PROOF_SUBMISSION_KEY: &str = "MUSUBI_KILL_SWITCH_PROOF_SUBMISSION";
const KILL_SWITCH_REALM_REQUESTS_KEY: &str = "MUSUBI_KILL_SWITCH_REALM_REQUESTS";
const KILL_SWITCH_REALM_ADMISSIONS_KEY: &str = "MUSUBI_KILL_SWITCH_REALM_ADMISSIONS";
const KILL_SWITCH_APPEAL_CREATION_KEY: &str = "MUSUBI_KILL_SWITCH_APPEAL_CREATION";

#[derive(Clone)]
pub struct LaunchPostureService {
    config: Arc<RwLock<LaunchPostureConfig>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    Closed,
    Pilot,
    Paused,
    OpenPreview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchAction {
    Auth,
    PromiseCreation,
    ProofChallenge,
    ProofSubmission,
    RealmRequest,
    RealmAdmission,
    AppealCreation,
}

#[derive(Clone, Debug)]
pub struct LaunchPostureConfig {
    mode: LaunchMode,
    allowlist_pi_uids: BTreeSet<String>,
    allowlist_account_ids: BTreeSet<String>,
    support_contact: Option<LaunchSupportContact>,
    kill_switches: KillSwitchSnapshot,
    config_warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LaunchSupportContact {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct KillSwitchSnapshot {
    pub auth: bool,
    pub promise_creation: bool,
    pub proof_challenge: bool,
    pub proof_submission: bool,
    pub realm_requests: bool,
    pub realm_admissions: bool,
    pub appeal_creation: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LaunchPostureSnapshot {
    pub launch_mode: String,
    pub participant_posture: String,
    pub message_code: String,
    pub support_contact: Option<LaunchSupportContact>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InternalLaunchPostureSnapshot {
    pub launch_mode: String,
    pub effective_posture: String,
    pub config_warnings: Vec<String>,
    pub kill_switches: KillSwitchSnapshot,
    pub allowlist: LaunchAllowlistSnapshot,
    pub support_contact_configured: bool,
    pub observability_is_launch_truth: bool,
    pub projection_is_launch_truth: bool,
    pub generated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LaunchAllowlistSnapshot {
    pub source: String,
    pub pi_uid_count: usize,
    pub account_id_count: usize,
    pub members_visible: bool,
}

#[derive(Clone, Debug)]
pub struct LaunchBlock {
    pub kind: LaunchBlockKind,
    pub message_code: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchBlockKind {
    Forbidden,
    ServiceUnavailable,
}

impl LaunchPostureService {
    pub fn from_env() -> Self {
        Self::new(LaunchPostureConfig::from_env())
    }

    pub fn new(config: LaunchPostureConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    pub(crate) async fn replace_config_for_test(&self, config: LaunchPostureConfig) {
        *self.config.write().await = config;
    }

    pub async fn public_snapshot(&self) -> LaunchPostureSnapshot {
        self.config.read().await.public_snapshot()
    }

    pub async fn internal_snapshot(&self) -> InternalLaunchPostureSnapshot {
        self.config.read().await.internal_snapshot()
    }

    pub(crate) async fn config_snapshot_for_check(&self) -> LaunchPostureConfig {
        self.config.read().await.clone()
    }

    pub async fn check_pi_auth(
        &self,
        pi_uid: &str,
        account_id: Option<&str>,
    ) -> Result<(), LaunchBlock> {
        self.config
            .read()
            .await
            .check_pi_identity_action(LaunchAction::Auth, pi_uid, account_id)
    }

    pub async fn check_participant_action(
        &self,
        action: LaunchAction,
        account_id: &str,
        pi_uid: Option<&str>,
    ) -> Result<(), LaunchBlock> {
        self.config
            .read()
            .await
            .check_participant_action(action, account_id, pi_uid)
    }
}

impl LaunchPostureConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let mut config_warnings = Vec::new();
        let mode = parse_mode(lookup(MODE_KEY).as_deref(), &mut config_warnings);
        let kill_switches = KillSwitchSnapshot {
            auth: parse_bool_switch(
                KILL_SWITCH_AUTH_KEY,
                lookup(KILL_SWITCH_AUTH_KEY).as_deref(),
                &mut config_warnings,
            ),
            promise_creation: parse_bool_switch(
                KILL_SWITCH_PROMISE_CREATION_KEY,
                lookup(KILL_SWITCH_PROMISE_CREATION_KEY).as_deref(),
                &mut config_warnings,
            ),
            proof_challenge: parse_bool_switch(
                KILL_SWITCH_PROOF_CHALLENGE_KEY,
                lookup(KILL_SWITCH_PROOF_CHALLENGE_KEY).as_deref(),
                &mut config_warnings,
            ),
            proof_submission: parse_bool_switch(
                KILL_SWITCH_PROOF_SUBMISSION_KEY,
                lookup(KILL_SWITCH_PROOF_SUBMISSION_KEY).as_deref(),
                &mut config_warnings,
            ),
            realm_requests: parse_bool_switch(
                KILL_SWITCH_REALM_REQUESTS_KEY,
                lookup(KILL_SWITCH_REALM_REQUESTS_KEY).as_deref(),
                &mut config_warnings,
            ),
            realm_admissions: parse_bool_switch(
                KILL_SWITCH_REALM_ADMISSIONS_KEY,
                lookup(KILL_SWITCH_REALM_ADMISSIONS_KEY).as_deref(),
                &mut config_warnings,
            ),
            appeal_creation: parse_bool_switch(
                KILL_SWITCH_APPEAL_CREATION_KEY,
                lookup(KILL_SWITCH_APPEAL_CREATION_KEY).as_deref(),
                &mut config_warnings,
            ),
        };
        let support_contact = parse_support_contact(
            lookup(SUPPORT_CONTACT_LABEL_KEY),
            lookup(SUPPORT_CONTACT_URL_KEY),
        );

        Self {
            mode,
            allowlist_pi_uids: parse_allowlist(lookup(ALLOWLIST_PI_UIDS_KEY)),
            allowlist_account_ids: parse_account_id_allowlist(
                lookup(ALLOWLIST_ACCOUNT_IDS_KEY),
                &mut config_warnings,
            ),
            support_contact,
            kill_switches,
            config_warnings,
        }
    }

    pub fn open_preview_for_test() -> Self {
        // Test-only bypass posture used by integration tests. Production env
        // parsing intentionally rejects `open_preview` for the Day 1 launch.
        Self {
            mode: LaunchMode::OpenPreview,
            allowlist_pi_uids: BTreeSet::new(),
            allowlist_account_ids: BTreeSet::new(),
            support_contact: None,
            kill_switches: KillSwitchSnapshot::default(),
            config_warnings: Vec::new(),
        }
    }

    pub fn closed_for_test() -> Self {
        Self {
            mode: LaunchMode::Closed,
            allowlist_pi_uids: BTreeSet::new(),
            allowlist_account_ids: BTreeSet::new(),
            support_contact: None,
            kill_switches: KillSwitchSnapshot::default(),
            config_warnings: Vec::new(),
        }
    }

    pub fn pilot_for_test(pi_uids: &[&str], account_ids: &[&str]) -> Self {
        Self {
            mode: LaunchMode::Pilot,
            allowlist_pi_uids: pi_uids.iter().map(|value| (*value).to_owned()).collect(),
            allowlist_account_ids: account_ids
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            support_contact: None,
            kill_switches: KillSwitchSnapshot::default(),
            config_warnings: Vec::new(),
        }
    }

    pub fn paused_for_test() -> Self {
        Self {
            mode: LaunchMode::Paused,
            allowlist_pi_uids: BTreeSet::new(),
            allowlist_account_ids: BTreeSet::new(),
            support_contact: None,
            kill_switches: KillSwitchSnapshot::default(),
            config_warnings: Vec::new(),
        }
    }

    pub fn with_kill_switch_for_test(action: LaunchAction) -> Self {
        let mut config = Self::open_preview_for_test();
        match action {
            LaunchAction::Auth => config.kill_switches.auth = true,
            LaunchAction::PromiseCreation => config.kill_switches.promise_creation = true,
            LaunchAction::ProofChallenge => config.kill_switches.proof_challenge = true,
            LaunchAction::ProofSubmission => config.kill_switches.proof_submission = true,
            LaunchAction::RealmRequest => config.kill_switches.realm_requests = true,
            LaunchAction::RealmAdmission => config.kill_switches.realm_admissions = true,
            LaunchAction::AppealCreation => config.kill_switches.appeal_creation = true,
        }
        config
    }

    pub fn public_snapshot(&self) -> LaunchPostureSnapshot {
        LaunchPostureSnapshot {
            launch_mode: self.mode.as_str().to_owned(),
            participant_posture: self.participant_posture().to_owned(),
            message_code: self.public_message_code().to_owned(),
            support_contact: self.support_contact.clone(),
            generated_at: Utc::now(),
        }
    }

    pub fn internal_snapshot(&self) -> InternalLaunchPostureSnapshot {
        InternalLaunchPostureSnapshot {
            launch_mode: self.mode.as_str().to_owned(),
            effective_posture: self.participant_posture().to_owned(),
            config_warnings: self.config_warnings.clone(),
            kill_switches: self.kill_switches.clone(),
            allowlist: LaunchAllowlistSnapshot {
                source: self.allowlist_source().to_owned(),
                pi_uid_count: self.allowlist_pi_uids.len(),
                account_id_count: self.allowlist_account_ids.len(),
                members_visible: false,
            },
            support_contact_configured: self.support_contact.is_some(),
            observability_is_launch_truth: false,
            projection_is_launch_truth: false,
            generated_at: Utc::now(),
        }
    }

    fn check_pi_identity_action(
        &self,
        action: LaunchAction,
        pi_uid: &str,
        account_id: Option<&str>,
    ) -> Result<(), LaunchBlock> {
        if let Some(block) = self.kill_switch_block(action) {
            return Err(block);
        }
        match self.mode {
            LaunchMode::OpenPreview => Ok(()),
            LaunchMode::Paused => Err(block_service_unavailable("launch_paused")),
            LaunchMode::Closed => Err(block_forbidden("launch_closed")),
            LaunchMode::Pilot => {
                if self.allowlist_pi_uids.contains(pi_uid)
                    || account_id
                        .is_some_and(|account_id| self.allowlist_account_ids.contains(account_id))
                {
                    Ok(())
                } else {
                    Err(block_forbidden("launch_pilot_not_allowed"))
                }
            }
        }
    }

    pub(crate) fn check_participant_action(
        &self,
        action: LaunchAction,
        account_id: &str,
        pi_uid: Option<&str>,
    ) -> Result<(), LaunchBlock> {
        if let Some(block) = self.kill_switch_block(action) {
            return Err(block);
        }
        match self.mode {
            LaunchMode::OpenPreview => Ok(()),
            LaunchMode::Paused => Err(block_service_unavailable("launch_paused")),
            LaunchMode::Closed => Err(block_forbidden("launch_closed")),
            LaunchMode::Pilot => {
                if self.allowlist_account_ids.contains(account_id)
                    || pi_uid.is_some_and(|pi_uid| self.allowlist_pi_uids.contains(pi_uid))
                {
                    Ok(())
                } else {
                    Err(block_forbidden("launch_pilot_not_allowed"))
                }
            }
        }
    }

    fn kill_switch_block(&self, action: LaunchAction) -> Option<LaunchBlock> {
        let message_code = match action {
            LaunchAction::Auth if self.kill_switches.auth => "auth_paused",
            LaunchAction::PromiseCreation if self.kill_switches.promise_creation => {
                "promise_creation_paused"
            }
            LaunchAction::ProofChallenge if self.kill_switches.proof_challenge => {
                "proof_challenge_paused"
            }
            LaunchAction::ProofSubmission if self.kill_switches.proof_submission => {
                "proof_submission_paused"
            }
            LaunchAction::RealmRequest if self.kill_switches.realm_requests => {
                "realm_request_paused"
            }
            LaunchAction::RealmAdmission if self.kill_switches.realm_admissions => {
                "realm_admission_paused"
            }
            LaunchAction::AppealCreation if self.kill_switches.appeal_creation => {
                "appeal_creation_paused"
            }
            _ => return None,
        };
        Some(block_service_unavailable(message_code))
    }

    fn participant_posture(&self) -> &'static str {
        match self.mode {
            LaunchMode::Closed => "closed",
            LaunchMode::Pilot => "pilot_only",
            LaunchMode::Paused => "paused",
            LaunchMode::OpenPreview => "available",
        }
    }

    fn public_message_code(&self) -> &'static str {
        match self.mode {
            LaunchMode::Closed => "launch_closed",
            LaunchMode::Pilot => "launch_pilot_not_allowed",
            LaunchMode::Paused => "launch_paused",
            LaunchMode::OpenPreview => "launch_available",
        }
    }

    fn allowlist_source(&self) -> &'static str {
        if self.allowlist_pi_uids.is_empty() && self.allowlist_account_ids.is_empty() {
            "none"
        } else {
            "env"
        }
    }
}

impl LaunchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Pilot => "pilot",
            Self::Paused => "paused",
            Self::OpenPreview => "open_preview",
        }
    }
}

fn parse_mode(value: Option<&str>, warnings: &mut Vec<String>) -> LaunchMode {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return LaunchMode::Closed;
    };
    match value {
        "closed" => LaunchMode::Closed,
        "pilot" => LaunchMode::Pilot,
        "paused" => LaunchMode::Paused,
        "open_preview" => {
            warnings.push("unsupported_launch_mode:open_preview".to_owned());
            LaunchMode::Closed
        }
        _ => {
            warnings.push("invalid_launch_mode".to_owned());
            LaunchMode::Closed
        }
    }
}

fn parse_bool_switch(key: &str, value: Option<&str>, warnings: &mut Vec<String>) -> bool {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => {
            warnings.push(format!("invalid_boolean_switch:{key}"));
            true
        }
    }
}

fn parse_allowlist(value: Option<String>) -> BTreeSet<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_account_id_allowlist(
    value: Option<String>,
    warnings: &mut Vec<String>,
) -> BTreeSet<String> {
    let mut account_ids = BTreeSet::new();
    for entry in value.unwrap_or_default().split(',').map(str::trim) {
        if entry.is_empty() {
            continue;
        }
        match Uuid::parse_str(entry) {
            Ok(account_id) => {
                account_ids.insert(account_id.to_string());
            }
            Err(_) => warnings.push("invalid_account_id_allowlist_entry".to_owned()),
        }
    }
    account_ids
}

fn parse_support_contact(
    label: Option<String>,
    url: Option<String>,
) -> Option<LaunchSupportContact> {
    let label = label.unwrap_or_default().trim().to_owned();
    let url = url.unwrap_or_default().trim().to_owned();
    if label.is_empty() || url.is_empty() {
        return None;
    }
    Some(LaunchSupportContact { label, url })
}

fn block_forbidden(message_code: &'static str) -> LaunchBlock {
    LaunchBlock {
        kind: LaunchBlockKind::Forbidden,
        message_code,
    }
}

fn block_service_unavailable(message_code: &'static str) -> LaunchBlock {
    LaunchBlock {
        kind: LaunchBlockKind::ServiceUnavailable,
        message_code,
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{LaunchAction, LaunchMode, LaunchPostureConfig};

    #[test]
    fn missing_launch_mode_defaults_closed() {
        let config = LaunchPostureConfig::from_lookup(|_| None);
        assert_eq!(config.mode, LaunchMode::Closed);
    }

    #[test]
    fn invalid_launch_mode_fails_closed_with_warning() {
        let config = LaunchPostureConfig::from_lookup(|name| {
            (name == "MUSUBI_LAUNCH_MODE").then(|| "public".to_owned())
        });
        assert_eq!(config.mode, LaunchMode::Closed);
        assert_eq!(config.config_warnings, vec!["invalid_launch_mode"]);
    }

    #[test]
    fn account_id_allowlist_normalizes_uuid_entries_and_warns_invalid() {
        let account_id = Uuid::new_v4();
        let config = LaunchPostureConfig::from_lookup(|name| match name {
            "MUSUBI_LAUNCH_MODE" => Some("pilot".to_owned()),
            "MUSUBI_LAUNCH_ALLOWLIST_ACCOUNT_IDS" => Some(format!(
                "{},not-a-uuid",
                account_id.to_string().to_uppercase()
            )),
            _ => None,
        });

        assert_eq!(config.allowlist_account_ids.len(), 1);
        assert!(
            config
                .allowlist_account_ids
                .contains(&account_id.to_string())
        );
        assert!(
            config
                .config_warnings
                .contains(&"invalid_account_id_allowlist_entry".to_owned())
        );
        assert!(
            config
                .check_participant_action(
                    LaunchAction::RealmRequest,
                    &account_id.to_string(),
                    None,
                )
                .is_ok()
        );
    }
}
