//! Asset-aware amount type.
//!
//! An [`Amount`] bundles a scaled integer value with the [`Asset`] it belongs
//! to, enabling precision-aware formatting at construction time rather than
//! deep inside the transaction builder.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::asset::Asset;

/// A quantity of a specific asset, stored as a scaled `i128`.
///
/// Created via [`Asset::try_amount`] (from raw scaled integer) or
/// [`Asset::parse_amount`] (from a decimal string like `"10.50"`).
/// All amounts are signed — negative values are allowed for debt modeling.
///
/// ```
/// # use ledger_core::Asset;
/// let usd = Asset::new("usd", 2);
/// let ten_bucks = usd.try_amount(1050);
/// assert_eq!(ten_bucks.to_string(), "10.50");
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
    pub(crate) fn new(asset: Asset, raw: i128) -> Self {
        Self { asset, raw }
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
        let raw = self.raw;
        let precision = self.asset.precision();
        if precision == 0 {
            return write!(f, "{raw}");
        }
        let scale = 10_i128.pow(precision as u32);
        let sign = if raw < 0 { "-" } else { "" };
        let abs = raw.unsigned_abs();
        let whole = abs / scale as u128;
        let frac = abs % scale as u128;
        write!(
            f,
            "{sign}{whole}.{frac:0>width$}",
            width = precision as usize
        )
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
        Ok(Amount::new(asset, wire.raw))
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
        let amt = Amount::new(usd(), -1050);
        assert_eq!(amt.raw(), -1050);
        assert_eq!(amt.to_string(), "-10.50");
    }

    #[test]
    fn allows_negative_any_asset() {
        let amt = Amount::new(brush(), -5);
        assert_eq!(amt.raw(), -5);
    }

    #[test]
    fn parse_valid() {
        let amt = usd().parse_qty("10.50").unwrap();
        assert_eq!(amt.raw(), 1050);
        assert_eq!(amt.asset_name(), "usd");
    }

    #[test]
    fn parse_invalid_precision() {
        assert!(usd().parse_qty("10.5").is_err());
    }

    #[test]
    fn negate() {
        let amt = Amount::new(usd(), 1050);
        let neg = amt.negate();
        assert_eq!(neg.raw(), -1050);
    }

    #[test]
    fn negate_zero() {
        let amt = Amount::new(brush(), 0);
        let neg = amt.negate();
        assert_eq!(neg.raw(), 0);
    }

    #[test]
    fn negate_any_asset() {
        let amt = Amount::new(brush(), 5);
        let neg = amt.negate();
        assert_eq!(neg.raw(), -5);
    }

    #[test]
    fn display_format() {
        let amt = Amount::new(usd(), 1050);
        // Display only shows the decimal value, no asset name
        assert_eq!(amt.to_string(), "10.50");
    }

    #[test]
    fn serde_roundtrip() {
        let amt = Amount::new(usd(), 1050);
        let json = serde_json::to_string(&amt).unwrap();
        let restored: Amount = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.raw(), 1050);
        assert_eq!(restored.asset_name(), "usd");
        assert_eq!(restored.asset().precision(), 2);
    }
}
