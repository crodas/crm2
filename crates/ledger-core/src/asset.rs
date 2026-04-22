//! Asset definitions and classification.
//!
//! An asset is any named, movable quantity with a declared decimal precision.
//! Assets are classified as either **signed** (monetary — can carry negative
//! quantities for debt modeling) or **unsigned** (physical — always >= 0).
//!
//! | Asset        | Kind     | Precision | Example qty |
//! |--------------|----------|-----------|-------------|
//! | `brush`      | Unsigned | 0         | `5`         |
//! | `usd`        | Signed   | 2         | `10.00`     |
//! | `cement_kg`  | Unsigned | 3         | `250.000`   |

use serde::{Deserialize, Serialize};
use std::fmt;

/// Whether an asset allows negative quantities.
///
/// - **Signed** assets (monetary) can carry negative quantities, enabling
///   debt/receivable modeling via the signed-position model.
/// - **Unsigned** assets (physical goods) must always be >= 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    /// Monetary asset — supports negative quantities for debt.
    Signed,
    /// Physical asset — quantities must be non-negative.
    Unsigned,
}

/// An asset definition with a name, decimal precision, and signedness.
///
/// ```
/// # use ledger_core::{Asset, AssetKind};
/// let usd = Asset::new("usd", 2, AssetKind::Signed);
/// assert_eq!(usd.precision(), 2);
/// assert_eq!(usd.kind(), AssetKind::Signed);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    name: String,
    precision: u8,
    kind: AssetKind,
}

impl Asset {
    /// Create a new asset definition.
    ///
    /// - `name` — unique identifier (e.g. `"usd"`, `"brush"`).
    /// - `precision` — number of decimal places (0 for whole units).
    /// - `kind` — whether the asset supports negative quantities.
    pub fn new(name: impl Into<String>, precision: u8, kind: AssetKind) -> Self {
        Self {
            name: name.into(),
            precision,
            kind,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn precision(&self) -> u8 {
        self.precision
    }

    pub fn kind(&self) -> AssetKind {
        self.kind
    }

    /// Create a validated [`Amount`] from a raw scaled integer.
    ///
    /// Returns `Err(NegativeUnsigned)` if the asset is unsigned and `raw < 0`.
    ///
    /// ```
    /// # use ledger_core::{Asset, AssetKind};
    /// let usd = Asset::new("usd", 2, AssetKind::Signed);
    /// let amt = usd.try_amount(1050).unwrap();
    /// assert_eq!(amt.raw(), 1050);
    ///
    /// let brush = Asset::new("brush", 0, AssetKind::Unsigned);
    /// assert!(brush.try_amount(-1).is_err());
    /// ```
    pub fn try_amount(&self, raw: i128) -> Result<crate::Amount, crate::LedgerError> {
        crate::Amount::new(self.clone(), raw)
    }

    /// Convert a scaled integer amount to its decimal string representation.
    ///
    /// Amounts are stored internally as integers scaled by `10^precision`
    /// (e.g., 1050 cents → `"10.50"` for a precision-2 asset). This method
    /// converts back to the human-readable decimal form expected by the
    /// transaction builder.
    ///
    /// ```
    /// # use ledger_core::{Asset, AssetKind};
    /// let usd = Asset::new("usd", 2, AssetKind::Signed);
    /// assert_eq!(usd.from_cents(1050), "10.50");
    /// assert_eq!(usd.from_cents(-1050), "-10.50");
    ///
    /// let brush = Asset::new("brush", 0, AssetKind::Unsigned);
    /// assert_eq!(brush.from_cents(5), "5");
    /// ```
    pub fn from_cents(&self, raw: i128) -> String {
        if self.precision == 0 {
            return raw.to_string();
        }
        let scale = 10_i128.pow(self.precision as u32);
        let sign = if raw < 0 { "-" } else { "" };
        let abs = raw.unsigned_abs();
        let whole = abs / scale as u128;
        let frac = abs % scale as u128;
        format!(
            "{sign}{whole}.{frac:0>width$}",
            width = self.precision as usize
        )
    }

    /// Parse a decimal string into the scaled integer representation.
    ///
    /// ```
    /// # use ledger_core::{Asset, AssetKind};
    /// let usd = Asset::new("usd", 2, AssetKind::Signed);
    /// assert_eq!(usd.parse_qty("10.50").unwrap(), 1050);
    /// assert_eq!(usd.parse_qty("-10.00").unwrap(), -1000);
    ///
    /// let brush = Asset::new("brush", 0, AssetKind::Unsigned);
    /// assert_eq!(brush.parse_qty("5").unwrap(), 5);
    /// ```
    pub fn parse_qty(&self, s: &str) -> Result<i128, ParseQtyError> {
        let negative = s.starts_with('-');
        let s = s.strip_prefix('-').unwrap_or(s);

        let scale = 10_i128.pow(self.precision as u32);

        let raw = if self.precision == 0 {
            if s.contains('.') {
                return Err(ParseQtyError::UnexpectedDecimal);
            }
            s.parse::<i128>().map_err(|_| ParseQtyError::Invalid)?
        } else {
            let (whole, frac) = if let Some((w, f)) = s.split_once('.') {
                if f.len() != self.precision as usize {
                    return Err(ParseQtyError::WrongPrecision {
                        expected: self.precision,
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

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
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
        let usd = Asset::new("usd", 2, AssetKind::Signed);
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
        let brush = Asset::new("brush", 0, AssetKind::Unsigned);
        assert_eq!(brush.from_cents(42), "42");
        assert_eq!(brush.parse_qty("42").expect("parse whole number"), 42);
        assert!(brush.parse_qty("4.2").is_err());
    }

    #[test]
    fn wrong_precision_rejected() {
        let usd = Asset::new("usd", 2, AssetKind::Signed);
        assert!(usd.parse_qty("10.0").is_err());
        assert!(usd.parse_qty("10.000").is_err());
        assert!(usd.parse_qty("10").is_err());
    }
}
