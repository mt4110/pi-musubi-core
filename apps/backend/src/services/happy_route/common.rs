use musubi_settlement_domain::{BackendError, CurrencyCode, Money};

use super::{
    constants::{PI_CURRENCY_CODE, PI_SCALE},
    types::HappyRouteError,
};

pub(super) fn canonical_pi_money(
    minor_units: i128,
    currency_code: &str,
) -> Result<Money, HappyRouteError> {
    if minor_units <= 0 {
        return Err(HappyRouteError::BadRequest(
            "deposit_amount_minor_units must be greater than zero".to_owned(),
        ));
    }

    let currency = CurrencyCode::new(currency_code).map_err(|_| {
        HappyRouteError::BadRequest("currency_code must be a valid settlement currency".to_owned())
    })?;
    if currency.as_str() != PI_CURRENCY_CODE {
        return Err(HappyRouteError::BadRequest(
            "Day 1 happy route only supports PI settlement".to_owned(),
        ));
    }

    Ok(Money::new(currency, minor_units, PI_SCALE))
}

pub(super) fn map_backend_error(error: BackendError) -> HappyRouteError {
    HappyRouteError::Internal(format!("settlement backend error: {error:?}"))
}
