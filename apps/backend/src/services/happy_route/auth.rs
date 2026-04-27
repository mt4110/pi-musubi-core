use crate::SharedState;

use super::types::{AuthenticatedAccount, AuthenticationInput, HappyRouteError};

pub async fn authenticate_pi_account(
    state: &SharedState,
    input: AuthenticationInput,
) -> Result<AuthenticatedAccount, HappyRouteError> {
    state.happy_route.authenticate_pi_account(input).await
}

pub async fn find_account_id_by_pi_uid(
    state: &SharedState,
    pi_uid: &str,
) -> Result<Option<String>, HappyRouteError> {
    state.happy_route.find_account_id_by_pi_uid(pi_uid).await
}

pub async fn find_account_id_by_pi_uid_if_access_token_matches(
    state: &SharedState,
    pi_uid: &str,
    access_token: &str,
) -> Result<Option<String>, HappyRouteError> {
    state
        .happy_route
        .find_account_id_by_pi_uid_if_access_token_matches(pi_uid, access_token)
        .await
}

pub async fn authorize_account(
    state: &SharedState,
    token: &str,
) -> Result<AuthenticatedAccount, HappyRouteError> {
    state.happy_route.authorize_account(token).await
}
