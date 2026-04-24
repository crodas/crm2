//! Asset-aware amount type.
//!
//! An [`Amount`] bundles a scaled integer value with the [`Asset`] it belongs
//! to, enabling precision-aware formatting at construction time rather than
//! deep inside the transaction builder.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::asset::Asset;
use crate::error::LedgerError;

/// A quantity of a specific asset, stored as a scaled `i128`.
///
/// Created via [`Asset::try_amount`] (from raw scaled integer) or
/// [`Asset::parse_amount`] (from a decimal string like `"10.50"`).
/// All amounts are signed — negative values are allowed for debt modeling.
///
/// ```
/// # use ledger_core::Asset;
/// let usd = Asset::new("usd", 2);
/// let ten_bucks = usd.try_amount(1050).unwrap();
/// assert_eq!(ten_bucks.to_decimal_string(), "10.50");
/// assert_eq!(ten_bucks.raw(), 1050);
/// assert_eq!(ten_bucks.asset_name(), "usd");
/// ```
#[derive(Debug, Clone)]
pub struct Amount {
    asset: Asset,
    raw: i128,
}

impl Amount {
    /// Create a new amount for the given asset.
    ///
    /// Prefer using [`Asset::try_amount`] instead.
    pub(crate) fn new(asset: Asset, raw: i128) -> Result<Self, LedgerError> {
        Ok(Self { asset, raw })
    }

    /// Create without sign validation.
    ///
    /// Use when the value is already known to be valid (e.g. read from storage).
    pub(crate) fn new_unchecked(asset: Asset, raw: i128) -> Self {
        Self { asset, raw }
    }

    /// Parse a decimal string into an Amount.
    ///
    /// Prefer using [`Asset::parse_amount`] instead.
    pub(crate) fn parse(asset: Asset, s: &str) -> Result<Self, LedgerError> {
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

    /// Negate the amount (for debt modeling / issuance).
    pub fn negate(&self) -> Self {
        Self {
            asset: self.asset.clone(),
            raw: -self.raw,
        }
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.to_decimal_string(), self.asset.name())
    }
}

/// Wire format for Amount serialization.
///
/// Includes full asset metadata so Amount can be reconstructed
/// without access to the asset registry.
#[derive(Serialize, Deserialize)]
struct AmountWire {
    asset_name: String,
    precision: u8,
    raw: i128,
}

impl Serialize for Amount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = AmountWire {
            asset_name: self.asset.name().to_string(),
            precision: self.asset.precision(),
            raw: self.raw,
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = AmountWire::deserialize(deserializer)?;
        let asset = Asset::new(wire.asset_name, wire.precision);
        Ok(Amount::new_unchecked(asset, wire.raw))
    }
}

impl PartialEq for Amount {
    fn eq(&self, other: &Self) -> bool {
        self.asset.name() == other.asset.name() && self.raw == other.raw
    }
}

impl Eq for Amount {}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd() -> Asset {
        Asset::new("usd", 2)
    }

    fn brush() -> Asset {
        Asset::new("brush", 0)
    }

    #[test]
    fn allows_negative() {
        let amt = Amount::new(usd(), -1050).unwrap();
        assert_eq!(amt.raw(), -1050);
        assert_eq!(amt.to_decimal_string(), "-10.50");
    }

    #[test]
    fn allows_negative_any_asset() {
        let amt = Amount::new(brush(), -5).unwrap();
        assert_eq!(amt.raw(), -5);
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
    fn negate() {
        let amt = Amount::new(usd(), 1050).unwrap();
        let neg = amt.negate();
        assert_eq!(neg.raw(), -1050);
    }

    #[test]
    fn negate_zero() {
        let amt = Amount::new(brush(), 0).unwrap();
        let neg = amt.negate();
        assert_eq!(neg.raw(), 0);
    }

    #[test]
    fn negate_any_asset() {
        let amt = Amount::new(brush(), 5).unwrap();
        let neg = amt.negate();
        assert_eq!(neg.raw(), -5);
    }

    #[test]
    fn display_format() {
        let amt = Amount::new(usd(), 1050).unwrap();
        assert_eq!(format!("{amt}"), "10.50 usd");
    }

    #[test]
    fn serde_roundtrip() {
        let amt = Amount::new(usd(), 1050).unwrap();
        let json = serde_json::to_string(&amt).unwrap();
        let restored: Amount = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.raw(), 1050);
        assert_eq!(restored.asset_name(), "usd");
        assert_eq!(restored.asset().precision(), 2);
    }
}
