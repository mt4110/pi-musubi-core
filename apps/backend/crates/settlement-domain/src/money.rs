use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl Into<String>) -> Result<Self, CurrencyCodeError> {
        let value = value.into().trim().to_ascii_uppercase();

        if value.is_empty() {
            return Err(CurrencyCodeError::Empty);
        }

        if !value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
        {
            return Err(CurrencyCodeError::InvalidCharacter);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Money {
    currency: CurrencyCode,
    minor_units: i128,
    scale: u32,
}

impl Money {
    pub fn new(currency: CurrencyCode, minor_units: i128, scale: u32) -> Self {
        Self {
            currency,
            minor_units,
            scale,
        }
    }

    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }

    pub const fn minor_units(&self) -> i128 {
        self.minor_units
    }

    pub const fn scale(&self) -> u32 {
        self.scale
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, MoneyError> {
        self.ensure_compatible(other)?;

        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or(MoneyError::Overflow)?;

        Ok(Self::new(self.currency.clone(), minor_units, self.scale))
    }

    pub fn checked_sub(&self, other: &Self) -> Result<Self, MoneyError> {
        self.ensure_compatible(other)?;

        let minor_units = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or(MoneyError::Overflow)?;

        Ok(Self::new(self.currency.clone(), minor_units, self.scale))
    }

    pub fn checked_cmp(&self, other: &Self) -> Result<Ordering, MoneyError> {
        self.ensure_compatible(other)?;
        Ok(self.minor_units.cmp(&other.minor_units))
    }

    fn ensure_compatible(&self, other: &Self) -> Result<(), MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch {
                left: self.currency.clone(),
                right: other.currency.clone(),
            });
        }

        if self.scale != other.scale {
            return Err(MoneyError::ScaleMismatch {
                left: self.scale,
                right: other.scale,
            });
        }

        Ok(())
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CurrencyCodeError {
    Empty,
    InvalidCharacter,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoneyError {
    CurrencyMismatch {
        left: CurrencyCode,
        right: CurrencyCode,
    },
    ScaleMismatch {
        left: u32,
        right: u32,
    },
    Overflow,
}
