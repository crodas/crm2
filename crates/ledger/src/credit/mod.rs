//! Pluggable credit strategies for the UTXO ledger.
//!
//! A [`CreditStrategy`] decides how to represent a credit (deposit/issuance)
//! within a transaction. Different implementations can produce different
//! account patterns — a simple cash deposit, a paypal receivable pair, etc.
//!
//! Strategies are configured with account templates at construction time.
//! The `{id}` placeholder is replaced with an entity identifier at call time.

mod template;

pub use template::TemplateCreditStrategy;

use ledger_core::Amount;

use crate::builder::TransactionBuilder;
use crate::error::Error;

/// Strategy for adding credit entries to ledger transactions.
///
/// Implementations decide which accounts to credit (and optionally debit
/// via negative credits) for a given entity and amount.
pub trait CreditStrategy: Send + Sync {
    /// Add credit entries to the transaction for `entity_id`.
    ///
    /// `amount` is always positive — the strategy decides the sign convention.
    fn apply(
        &self,
        builder: TransactionBuilder,
        entity_id: &str,
        amount: &Amount,
    ) -> Result<TransactionBuilder, Error>;
}
