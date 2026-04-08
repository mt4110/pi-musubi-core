use chrono::Utc;
use uuid::Uuid;

use crate::SharedState;

use super::{
    state::{AccountRecord, AuthSessionRecord},
    types::{AuthenticatedAccount, AuthenticationInput, HappyRouteError},
};

pub async fn authenticate_pi_account(
    state: &SharedState,
    input: AuthenticationInput,
) -> Result<AuthenticatedAccount, HappyRouteError> {
    let now = Utc::now();
    let mut store = state.happy_route.write().await;

    let account_id =
        if let Some(existing_account_id) = store.account_id_by_pi_uid.get(&input.pi_uid) {
            let account_id = existing_account_id.clone();
            if let Some(account) = store.accounts_by_id.get_mut(&account_id) {
                account.username = input.username.clone();
                account.wallet_address = input.wallet_address.clone();
                account.updated_at = now;
            }
            account_id
        } else {
            let account_id = Uuid::new_v4().to_string();
            let account = AccountRecord {
                account_id: account_id.clone(),
                pi_uid: input.pi_uid.clone(),
                username: input.username.clone(),
                wallet_address: input.wallet_address.clone(),
                created_at: now,
                updated_at: now,
            };

            store
                .account_id_by_pi_uid
                .insert(input.pi_uid.clone(), account_id.clone());
            store.accounts_by_id.insert(account_id.clone(), account);
            account_id
        };

    let token = format!("pi-session-{}", Uuid::new_v4());
    let session = AuthSessionRecord {
        token: token.clone(),
        account_id: account_id.clone(),
        created_at: now,
    };
    store.auth_sessions_by_token.insert(token.clone(), session);

    Ok(AuthenticatedAccount {
        token,
        account_id,
        pi_uid: input.pi_uid,
        username: input.username,
    })
}

pub async fn authorize_account(
    state: &SharedState,
    token: &str,
) -> Result<AuthenticatedAccount, HappyRouteError> {
    let store = state.happy_route.read().await;
    let session = store.auth_sessions_by_token.get(token).ok_or_else(|| {
        HappyRouteError::Unauthorized("valid bearer token is required".to_owned())
    })?;
    let account = store
        .accounts_by_id
        .get(&session.account_id)
        .ok_or_else(|| {
            HappyRouteError::Internal(
                "session account is missing from authoritative state".to_owned(),
            )
        })?;

    Ok(AuthenticatedAccount {
        token: session.token.clone(),
        account_id: account.account_id.clone(),
        pi_uid: account.pi_uid.clone(),
        username: account.username.clone(),
    })
}
