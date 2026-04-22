//! Asset-aware amount type.
//!
//! An [`Amount`] bundles a scaled integer value with the [`Asset`] it belongs
//! to, enabling precision-aware formatting and kind-aware validation at
//! construction time rather than deep inside the transaction builder.

use std::fmt;

use crate::asset::{Asset, AssetKind};
use crate::error::LedgerError;

/// A quantity of a specific asset, stored as a scaled `i128`.
///
/// Created via [`Amount::new`] (from raw scaled integer) or [`Amount::parse`]
/// (from a decimal string like `"10.50"`). Construction validates that
/// unsigned assets never carry negative values.
///
/// ```
/// # use ledger_core::{Amount, Asset, AssetKind};
/// let usd = Asset::new("usd", 2, AssetKind::Signed);
/// let ten_bucks = Amount::new(usd.clone(), 1050).unwrap();
/// assert_eq!(ten_bucks.to_decimal_string(), "10.50");
/// assert_eq!(ten_bucks.raw(), 1050);
/// assert_eq!(ten_bucks.asset_name(), "usd");
///
/// let brush = Asset::new("brush", 0, AssetKind::Unsigned);
/// assert!(Amount::new(brush, -1).is_err()); // unsigned rejects negative
/// ```
#[derive(Debug, Clone)]
pub struct Amount {
    asset: Asset,
    raw: i128,
}

impl Amount {
    /// Create a new amount for the given asset.
    ///
    /// Returns `Err(NegativeUnsigned)` if the asset is unsigned and `raw < 0`.
    pub fn new(asset: Asset, raw: i128) -> Result<Self, LedgerError> {
        if asset.kind() == AssetKind::Unsigned && raw < 0 {
            return Err(LedgerError::NegativeUnsigned {
                asset: asset.name().to_string(),
                qty: raw,
            });
        }
        Ok(Self { asset, raw })
    }

    /// Create without sign validation.
    ///
    /// Use when the value is already known to be valid (e.g. read from storage).
    pub fn new_unchecked(asset: Asset, raw: i128) -> Self {
        Self { asset, raw }
    }

    /// Parse a decimal string into an Amount.
    ///
    /// ```
    /// # use ledger_core::{Amount, Asset, AssetKind};
    /// let usd = Asset::new("usd", 2, AssetKind::Signed);
    /// let amt = Amount::parse(usd, "10.50").unwrap();
    /// assert_eq!(amt.raw(), 1050);
    /// ```
    pub fn parse(asset: Asset, s: &str) -> Result<Self, LedgerError> {
        let raw = asset
            .parse_qty(s)
            .map_err(|_| LedgerError::InvalidQty(s.to_string()))?;
        Self::new(asset, raw)
    }

    /// The scaled integer value.
    pub fn raw(&self) -> i128 {
        self.raw
    }

    /// The asset this amount belongs to.
    pub fn asset(&self) -> &Asset {
        &self.asset
    }

    /// The asset name (convenience).
    pub fn asset_name(&self) -> &str {
        self.asset.name()
    }

    /// Format as a decimal string with correct precision.
    pub fn to_decimal_string(&self) -> String {
        self.asset.from_cents(self.raw)
    }

    /// Negate the amount (for debt modeling).
    ///
    /// Returns `Err` if the asset is unsigned and the result would be negative.
    pub fn negate(&self) -> Result<Self, LedgerError> {
        Self::new(self.asset.clone(), -self.raw)
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.to_decimal_string(), self.asset.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd() -> Asset {
        Asset::new("usd", 2, AssetKind::Signed)
    }

    fn brush() -> Asset {
        Asset::new("brush", 0, AssetKind::Unsigned)
    }

    #[test]
    fn new_signed_allows_negative() {
        let amt = Amount::new(usd(), -1050).unwrap();
        assert_eq!(amt.raw(), -1050);
        assert_eq!(amt.to_decimal_string(), "-10.50");
    }

    #[test]
    fn new_unsigned_rejects_negative() {
        assert!(Amount::new(brush(), -1).is_err());
    }

    #[test]
    fn parse_valid() {
        let amt = Amount::parse(usd(), "10.50").unwrap();
        assert_eq!(amt.raw(), 1050);
        assert_eq!(amt.asset_name(), "usd");
    }

    #[test]
    fn parse_invalid_precision() {
        assert!(Amount::parse(usd(), "10.5").is_err());
    }

    #[test]
    fn negate_signed() {
        let amt = Amount::new(usd(), 1050).unwrap();
        let neg = amt.negate().unwrap();
        assert_eq!(neg.raw(), -1050);
    }

    #[test]
    fn negate_unsigned_zero_ok() {
        let amt = Amount::new(brush(), 0).unwrap();
        let neg = amt.negate().unwrap();
        assert_eq!(neg.raw(), 0);
    }

    #[test]
    fn negate_unsigned_positive_fails() {
        let amt = Amount::new(brush(), 5).unwrap();
        assert!(amt.negate().is_err());
    }

    #[test]
    fn display_format() {
        let amt = Amount::new(usd(), 1050).unwrap();
        assert_eq!(format!("{amt}"), "10.50 usd");
    }
}
