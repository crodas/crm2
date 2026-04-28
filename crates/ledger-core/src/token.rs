//! Spending tokens — the fundamental unit of value in the ledger.
//!
//! A spending token represents a quantity of one asset owned by one account.
//! Once consumed by a transaction, it is permanently spent and cannot be
//! reused (UTXO model).

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::amount::Amount;

/// A reference to a specific entry within a committed transaction.
///
/// Debits reference prior entries by `(tx_id, entry_index)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntryRef {
    /// The transaction ID that created this entry.
    pub tx_id: String,
    /// Zero-based position within that transaction's credits.
    pub entry_index: u32,
}

impl fmt::Display for EntryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", &self.tx_id[..8], self.entry_index)
    }
}

/// The current status of a spending token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStatus {
    /// Unspent — available for consumption.
    Unspent,
    /// Spent by the transaction with this ID.
    Spent(/* spent_by_tx index */ usize),
}

/// A spending token stored in the ledger.
///
/// Each token is created as a credit of a transaction and can be consumed
/// exactly once as a debit in a later transaction.
#[derive(Debug, Clone)]
pub struct SpendingToken {
    /// Which transaction entry created this token.
    pub entry_ref: EntryRef,
    /// The account that owns this token.
    pub owner: String,
    /// The amount (asset + quantity).
    pub amount: Amount,
    /// Whether this token has been consumed.
    pub status: TokenStatus,
}
