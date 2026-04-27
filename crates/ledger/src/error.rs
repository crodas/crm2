//! High-level ledger errors.

use ledger_core::LedgerError;

/// Errors from the high-level ledger API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Not enough unspent credit tokens to cover the requested debit.
    #[error("insufficient balance for {account} / {asset}: need {required}, have {available}")]
    InsufficientBalance {
        account: String,
        asset: String,
        required: i128,
        available: i128,
    },

    /// Debt amount must be positive.
    #[error("debt amount must be positive")]
    NonPositiveAmount,

    /// Not enough debt credit tokens to cover the settlement.
    #[error("insufficient debt: need {required}, have {available}")]
    InsufficientDebt { required: i128, available: i128 },

    /// No debt strategy configured on this ledger.
    #[error("no debt strategy configured — call Ledger::with_debt_strategy first")]
    NoDebtStrategy,

    /// Invalid account path resolved from a template.
    #[error("invalid account path: {0}")]
    InvalidPath(String),

    /// Core ledger error.
    #[error(transparent)]
    Ledger(#[from] LedgerError),
}
