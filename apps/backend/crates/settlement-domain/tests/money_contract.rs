use musubi_settlement_domain::{CurrencyCode, Money, MoneyError};

fn currency(code: &str) -> CurrencyCode {
    CurrencyCode::new(code).expect("currency code must be valid in tests")
}

#[test]
fn checked_add_rejects_currency_mismatch() {
    let left = Money::new(currency("PI"), 1000, 3);
    let right = Money::new(currency("JPY"), 1000, 0);

    let result = left.checked_add(&right);

    assert_eq!(
        result,
        Err(MoneyError::CurrencyMismatch {
            left: currency("PI"),
            right: currency("JPY"),
        })
    );
}

#[test]
fn checked_sub_rejects_scale_mismatch() {
    let left = Money::new(currency("PI"), 1000, 3);
    let right = Money::new(currency("PI"), 1000, 6);

    let result = left.checked_sub(&right);

    assert_eq!(result, Err(MoneyError::ScaleMismatch { left: 3, right: 6 }));
}
