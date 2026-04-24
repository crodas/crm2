//! Pluggable issuance strategies for the UTXO ledger.
//!
//! An [`IssuanceStrategy`] decides how to represent token issuance within a
//! transaction — crediting the destination account and debiting a source
//! (e.g., `@world`) to maintain conservation.

mod template;

pub use template::TemplateIssuanceStrategy;

use ledger_core::Amount;

use crate::builder::TransactionBuilder;
use crate::error::Error;

/// Strategy for issuing new tokens into the ledger.
///
/// Implementations add balanced credit entries (positive + negative) so that
/// `sum(debits) == sum(credits)` holds for every transaction.
pub trait IssuanceStrategy: Send + Sync {
    /// Issue `amount` to the `to` account, adding balancing entries.
    ///
    /// `amount` is always positive — the strategy creates the negative
    /// counterpart at the source account.
    fn apply(
        &self,
        builder: TransactionBuilder,
        to: &str,
        amount: &Amount,
    ) -> Result<TransactionBuilder, Error>;
}
