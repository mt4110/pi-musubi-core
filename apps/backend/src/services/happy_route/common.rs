use musubi_settlement_domain::{BackendError, CurrencyCode, Money};

use super::{
    constants::{PI_CURRENCY_CODE, PI_SCALE},
    types::{HappyRouteError, ProviderErrorClass},
};

pub(super) fn canonical_pi_money(
    minor_units: i128,
    currency_code: &str,
) -> Result<Money, HappyRouteError> {
    if minor_units <= 0 {
        return Err(HappyRouteError::BadRequest(
            "minor_units must be greater than zero".to_owned(),
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
    let class = match error {
        BackendError::Timeout | BackendError::TemporarilyUnavailable => {
            ProviderErrorClass::Retryable
        }
        BackendError::InvalidProviderResponse | BackendError::ObservationNormalizationFailed => {
            ProviderErrorClass::ManualReview
        }
        BackendError::IdempotencyMappingFailed => ProviderErrorClass::ManualReview,
        BackendError::InvalidProviderPayload
        | BackendError::CapabilityUnsupported { .. }
        | BackendError::BackendPinMismatch { .. } => ProviderErrorClass::Terminal,
        _ => ProviderErrorClass::ManualReview,
    };

    HappyRouteError::Provider {
        class,
        message: format!("settlement backend error: {error:?}"),
    }
}

#[cfg(test)]
mod tests {
    use musubi_settlement_domain::BackendError;

    use super::*;

    #[test]
    fn backend_error_mapping_preserves_retry_classification() {
        assert_eq!(
            map_backend_error(BackendError::Timeout).provider_error_class(),
            Some(ProviderErrorClass::Retryable)
        );
        assert_eq!(
            map_backend_error(BackendError::IdempotencyMappingFailed).provider_error_class(),
            Some(ProviderErrorClass::ManualReview)
        );
        assert_eq!(
            map_backend_error(BackendError::InvalidProviderPayload).provider_error_class(),
            Some(ProviderErrorClass::Terminal)
        );
    }
}
