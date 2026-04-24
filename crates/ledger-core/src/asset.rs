//! Asset definitions.
//!
//! An asset is any named, movable quantity with a declared decimal precision.
//! All amounts are signed, supporting both positive and negative quantities
//! for debt/receivable modeling.
//!
//! | Asset        | Precision | Example qty |
//! |--------------|-----------|-------------|
//! | `brush`      | 0         | `5`         |
//! | `usd`        | 2         | `10.00`     |
//! | `cement_kg`  | 3         | `250.000`   |

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Inner data for an asset definition.
#[derive(Debug, PartialEq, Eq)]
struct AssetInner {
    name: String,
    precision: u8,
}

/// An asset definition with a name and decimal precision.
///
/// Cheap to clone (internally `Arc`-backed).
///
/// ```
/// # use ledger_core::Asset;
/// let usd = Asset::new("usd", 2);
/// assert_eq!(usd.precision(), 2);
/// ```
#[derive(Debug, Clone)]
pub struct Asset(Arc<AssetInner>);

impl Asset {
    /// Create a new asset definition.
    ///
    /// - `name` — unique identifier (e.g. `"usd"`, `"brush"`).
    /// - `precision` — number of decimal places (0 for whole units).
    pub fn new(name: impl Into<String>, precision: u8) -> Self {
        Self(Arc::new(AssetInner {
            name: name.into(),
            precision,
        }))
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn precision(&self) -> u8 {
        self.0.precision
    }

    /// Create an [`Amount`] from a raw scaled integer.
    ///
    /// ```
    /// # use ledger_core::Asset;
    /// let usd = Asset::new("usd", 2);
    /// let amt = usd.try_amount(1050).unwrap();
    /// assert_eq!(amt.raw(), 1050);
    /// ```
    pub fn try_amount(&self, raw: i128) -> Result<crate::Amount, crate::LedgerError> {
        crate::Amount::new(self.clone(), raw)
    }

    /// Create an [`Amount`] from a raw scaled integer without validation.
    ///
    /// Use when the value is already known to be valid (e.g. read from storage).
    pub fn amount_unchecked(&self, raw: i128) -> crate::Amount {
        crate::Amount::new_unchecked(self.clone(), raw)
    }

    /// Parse a decimal string into a validated [`Amount`].
    ///
    /// ```
    /// # use ledger_core::Asset;
    /// let usd = Asset::new("usd", 2);
    /// let amt = usd.parse_amount("10.50").unwrap();
    /// assert_eq!(amt.raw(), 1050);
    /// ```
    pub fn parse_amount(&self, s: &str) -> Result<crate::Amount, crate::LedgerError> {
        crate::Amount::parse(self.clone(), s)
    }

    /// Convert a scaled integer amount to its decimal string representation.
    ///
    /// Amounts are stored internally as integers scaled by `10^precision`
    /// (e.g., 1050 cents → `"10.50"` for a precision-2 asset). This method
    /// converts back to the human-readable decimal form expected by the
    /// transaction builder.
    ///
    /// ```
    /// # use ledger_core::Asset;
    /// let usd = Asset::new("usd", 2);
    /// assert_eq!(usd.from_cents(1050), "10.50");
    /// assert_eq!(usd.from_cents(-1050), "-10.50");
    ///
    /// let brush = Asset::new("brush", 0);
    /// assert_eq!(brush.from_cents(5), "5");
    /// ```
    pub fn from_cents(&self, raw: i128) -> String {
        if self.0.precision == 0 {
            return raw.to_string();
        }
        let scale = 10_i128.pow(self.0.precision as u32);
        let sign = if raw < 0 { "-" } else { "" };
        let abs = raw.unsigned_abs();
        let whole = abs / scale as u128;
        let frac = abs % scale as u128;
        format!(
            "{sign}{whole}.{frac:0>width$}",
            width = self.0.precision as usize
        )
    }

    /// Parse a decimal string into the scaled integer representation.
    ///
    /// ```
    /// # use ledger_core::Asset;
    /// let usd = Asset::new("usd", 2);
    /// assert_eq!(usd.parse_qty("10.50").unwrap(), 1050);
    /// assert_eq!(usd.parse_qty("-10.00").unwrap(), -1000);
    ///
    /// let brush = Asset::new("brush", 0);
    /// assert_eq!(brush.parse_qty("5").unwrap(), 5);
    /// ```
    pub fn parse_qty(&self, s: &str) -> Result<i128, ParseQtyError> {
        let negative = s.starts_with('-');
        let s = s.strip_prefix('-').unwrap_or(s);

        let scale = 10_i128.pow(self.0.precision as u32);

        let raw = if self.0.precision == 0 {
            if s.contains('.') {
                return Err(ParseQtyError::UnexpectedDecimal);
            }
            s.parse::<i128>().map_err(|_| ParseQtyError::Invalid)?
        } else {
            let (whole, frac) = if let Some((w, f)) = s.split_once('.') {
                if f.len() != self.0.precision as usize {
                    return Err(ParseQtyError::WrongPrecision {
                        expected: self.0.precision,
                        got: f.len() as u8,
                    });
                }
                (
                    w.parse::<i128>().map_err(|_| ParseQtyError::Invalid)?,
                    f.parse::<i128>().map_err(|_| ParseQtyError::Invalid)?,
                )
            } else {
                return Err(ParseQtyError::MissingDecimal);
            };
            whole * scale + frac
        };

        Ok(if negative { -raw } else { raw })
    }
}

impl PartialEq for Asset {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Asset {}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.name)
    }
}

// Custom serde: serialize as the inner fields, not as a newtype wrapper.
impl Serialize for Asset {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Asset", 2)?;
        s.serialize_field("name", &self.0.name)?;
        s.serialize_field("precision", &self.0.precision)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Asset {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct AssetData {
            name: String,
            precision: u8,
        }
        let data = AssetData::deserialize(deserializer)?;
        Ok(Asset::new(data.name, data.precision))
    }
}

/// Errors returned when parsing a quantity string.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseQtyError {
    #[error("invalid number")]
    Invalid,
    #[error("expected decimal point for asset with precision > 0")]
    MissingDecimal,
    #[error("unexpected decimal point for asset with precision 0")]
    UnexpectedDecimal,
    #[error("wrong number of decimal places: expected {expected}, got {got}")]
    WrongPrecision { expected: u8, got: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse_roundtrip() {
        let usd = Asset::new("usd", 2);
        for raw in [-1050, -100, 0, 100, 1050, 999999] {
            let s = usd.from_cents(raw);
            assert_eq!(
                usd.parse_qty(&s).expect("roundtrip parse should succeed"),
                raw,
                "roundtrip failed for {raw}"
            );
        }
    }

    #[test]
    fn zero_precision() {
        let brush = Asset::new("brush", 0);
        assert_eq!(brush.from_cents(42), "42");
        assert_eq!(brush.parse_qty("42").expect("parse whole number"), 42);
        assert!(brush.parse_qty("4.2").is_err());
    }

    #[test]
    fn wrong_precision_rejected() {
        let usd = Asset::new("usd", 2);
        assert!(usd.parse_qty("10.0").is_err());
        assert!(usd.parse_qty("10.000").is_err());
        assert!(usd.parse_qty("10").is_err());
    }

    #[test]
    fn cheap_clone() {
        let usd = Asset::new("usd", 2);
        let usd2 = usd.clone();
        assert_eq!(usd, usd2);
        // Arc means same pointer
        assert!(Arc::ptr_eq(&usd.0, &usd2.0));
    }

    #[test]
    fn serde_roundtrip() {
        let usd = Asset::new("usd", 2);
        let json = serde_json::to_string(&usd).unwrap();
        let restored: Asset = serde_json::from_str(&json).unwrap();
        assert_eq!(usd, restored);
    }
}
