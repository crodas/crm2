//! Account prefix matching utilities.
//!
//! Accounts are plain strings. The `/` separator enables hierarchical
//! prefix queries:
//!
//! ```text
//! store1
//! store1/inventory
//! store1/receivables/sale_1
//! customer1/cash
//! ```

/// Returns `true` if `other` is a descendant of (or equal to) `prefix`.
///
/// ```
/// # use ledger_core::is_prefix_of;
/// assert!(is_prefix_of("store1", "store1/inventory"));
/// assert!(is_prefix_of("store1", "store1"));
/// assert!(!is_prefix_of("store1", "store2"));
/// ```
pub fn is_prefix_of(prefix: &str, other: &str) -> bool {
    other == prefix || other.starts_with(&format!("{prefix}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_matching() {
        assert!(is_prefix_of("store1", "store1/inventory"));
        assert!(is_prefix_of("store1", "store1"));
        assert!(!is_prefix_of("store1", "store2"));
        assert!(!is_prefix_of("store1/inventory", "store1"));
    }
}
