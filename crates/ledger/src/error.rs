//! High-level ledger errors.

use ledger_core::LedgerError;

/// Errors from the high-level ledger API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Not enough unspent tokens to cover the requested debit.
    #[error("insufficient balance for {account} / {asset}: need {required}, have {available}")]
    InsufficientBalance {
        account: String,
        asset: String,
        required: i128,
        available: i128,
    },

    /// Core ledger error.
    #[error(transparent)]
    Ledger(#[from] LedgerError),
}
