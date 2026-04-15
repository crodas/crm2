//! Hierarchical account paths.
//!
//! An account is a named bucket that owns spending tokens, addressed as a
//! hierarchical path starting with `@`:
//!
//! ```text
//! @store1
//! @store1/inventory
//! @store1/receivables/sale_1
//! @customer1/cash
//! ```
//!
//! The hierarchy is a naming convention — the engine treats all paths as
//! equal. Hierarchy is enforced and interpreted at the query layer via
//! prefix matching.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A hierarchical account path (e.g. `@store1/inventory`).
///
/// Account paths must:
/// - Start with `@`
/// - Contain at least one character after the prefix
/// - Not be `@world` (reserved pseudo-account for issuance)
///
/// The path `@world` represents everything outside the ledger and is never
/// a valid owner of spending tokens.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AccountPath(String);

impl AccountPath {
    /// Create a new account path from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the path does not start with `@`, is empty after
    /// the prefix, or is the reserved `@world` path.
    pub fn new(path: impl Into<String>) -> Result<Self, InvalidAccountPath> {
        let path = path.into();
        if !path.starts_with('@') {
            return Err(InvalidAccountPath::MissingPrefix(path));
        }
        if path.len() < 2 {
            return Err(InvalidAccountPath::Empty);
        }
        if path == "@world" {
            return Err(InvalidAccountPath::ReservedWorld);
        }
        Ok(Self(path))
    }

    /// Returns the full path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` if `other` is a descendant of (or equal to) this path.
    ///
    /// ```
    /// # use ledger_core::AccountPath;
    /// let store = AccountPath::new("@store1").unwrap();
    /// let inv = AccountPath::new("@store1/inventory").unwrap();
    /// assert!(store.is_prefix_of(&inv));
    /// assert!(store.is_prefix_of(&store));
    /// ```
    pub fn is_prefix_of(&self, other: &AccountPath) -> bool {
        other.0 == self.0 || other.0.starts_with(&format!("{}/", self.0))
    }
}

impl fmt::Display for AccountPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<AccountPath> for String {
    fn from(p: AccountPath) -> String {
        p.0
    }
}

impl TryFrom<String> for AccountPath {
    type Error = InvalidAccountPath;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

/// Errors returned when constructing an [`AccountPath`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum InvalidAccountPath {
    #[error("account path must start with '@': {0}")]
    MissingPrefix(String),
    #[error("account path must have at least one character after '@'")]
    Empty,
    #[error("'@world' is a reserved pseudo-account and cannot own tokens")]
    ReservedWorld,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_paths() {
        assert!(AccountPath::new("@store1").is_ok());
        assert!(AccountPath::new("@store1/inventory").is_ok());
        assert!(AccountPath::new("@customer1/sale_1").is_ok());
    }

    #[test]
    fn rejects_invalid() {
        assert!(AccountPath::new("store1").is_err());
        assert!(AccountPath::new("@").is_err());
        assert!(AccountPath::new("@world").is_err());
    }

    #[test]
    fn prefix_matching() {
        let store = AccountPath::new("@store1").expect("valid path: @store1");
        let inv = AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        let other = AccountPath::new("@store2").expect("valid path: @store2");

        assert!(store.is_prefix_of(&inv));
        assert!(store.is_prefix_of(&store));
        assert!(!store.is_prefix_of(&other));
        assert!(!inv.is_prefix_of(&store));
    }
}
