//! Asset-aware amount type.
//!
//! An [`Amount`] bundles a scaled integer value with the [`Asset`] it belongs
//! to, enabling precision-aware formatting and kind-aware validation at
//! construction time rather than deep inside the transaction builder.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::asset::{Asset, AssetKind};
use crate::error::LedgerError;

/// A quantity of a specific asset, stored as a scaled `i128`.
///
/// Created via [`Asset::try_amount`] (from raw scaled integer) or
/// [`Asset::parse_amount`] (from a decimal string like `"10.50"`).
/// Construction validates that unsigned assets never carry negative values.
///
/// ```
/// # use ledger_core::{Asset, AssetKind};
/// let usd = Asset::new("usd", 2, AssetKind::Signed);
/// let ten_bucks = usd.try_amount(1050).unwrap();
/// assert_eq!(ten_bucks.to_decimal_string(), "10.50");
/// assert_eq!(ten_bucks.raw(), 1050);
/// assert_eq!(ten_bucks.asset_name(), "usd");
///
/// let brush = Asset::new("brush", 0, AssetKind::Unsigned);
/// assert!(brush.try_amount(-1).is_err()); // unsigned rejects negative
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
    ///
    /// Prefer using [`Asset::try_amount`] or [`Asset::amount`] instead.
    pub(crate) fn new(asset: Asset, raw: i128) -> Result<Self, LedgerError> {
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

/// Wire format for Amount serialization.
///
/// Includes full asset metadata so Amount can be reconstructed
/// without access to the asset registry.
#[derive(Serialize, Deserialize)]
struct AmountWire {
    asset_name: String,
    precision: u8,
    kind: AssetKind,
    raw: i128,
}

impl Serialize for Amount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = AmountWire {
            asset_name: self.asset.name().to_string(),
            precision: self.asset.precision(),
            kind: self.asset.kind(),
            raw: self.raw,
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = AmountWire::deserialize(deserializer)?;
        let asset = Asset::new(wire.asset_name, wire.precision, wire.kind);
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

    #[test]
    fn serde_roundtrip() {
        let amt = Amount::new(usd(), 1050).unwrap();
        let json = serde_json::to_string(&amt).unwrap();
        let restored: Amount = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.raw(), 1050);
        assert_eq!(restored.asset_name(), "usd");
        assert_eq!(restored.asset().precision(), 2);
        assert_eq!(restored.asset().kind(), AssetKind::Signed);
    }
}
