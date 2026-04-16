use crate::SharedState;

use super::types::{AuthenticatedAccount, AuthenticationInput, HappyRouteError};

pub async fn authenticate_pi_account(
    state: &SharedState,
    input: AuthenticationInput,
) -> Result<AuthenticatedAccount, HappyRouteError> {
    state.happy_route.authenticate_pi_account(input).await
}

pub async fn authorize_account(
    state: &SharedState,
    token: &str,
) -> Result<AuthenticatedAccount, HappyRouteError> {
    state.happy_route.authorize_account(token).await
}
