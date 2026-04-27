//! Credit tokens — the fundamental unit of value in the ledger.
//!
//! A credit token represents a quantity of one asset owned by one account.
//! Once consumed by a transaction, it is permanently spent and cannot be
//! reused (UTXO model).

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::amount::Amount;

/// A reference to a specific credit entry within a committed transaction.
///
/// Debits reference prior entries by `(tx_id, entry_index)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CreditEntryRef {
    /// The transaction ID that created this entry.
    pub tx_id: String,
    /// Zero-based position within that transaction's credits.
    pub entry_index: u32,
}

impl fmt::Display for CreditEntryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", &self.tx_id[..8], self.entry_index)
    }
}

/// The current status of a credit token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreditTokenStatus {
    /// Unspent — available for consumption.
    Unspent,
    /// Spent by the transaction at this index.
    Spent(/* spent_by_tx index */ usize),
}

/// A credit token stored in the ledger.
///
/// Each credit token is created by a credit entry in a transaction and can be
/// consumed exactly once as a debit in a later transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditToken {
    /// Which transaction entry created this credit token.
    pub entry_ref: CreditEntryRef,
    /// The account that owns this credit token.
    pub owner: String,
    /// The amount (asset + quantity).
    pub amount: Amount,
    /// Whether this credit token has been consumed.
    pub status: CreditTokenStatus,
}
