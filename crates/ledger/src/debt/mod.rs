//! Pluggable debt strategies for the UTXO ledger.
//!
//! The ledger itself has no concept of debt — it only enforces that both sides
//! of a transaction balance (debits == credits per asset). Debt is a
//! higher-level concern built on top of the ledger's conservation rules.
//!
//! The [`DebtStrategy`] trait provides helpers that inject a specific debt
//! representation into a transaction builder. Different implementations choose
//! different ways to represent obligations, but the ledger does not care as
//! long as debits and credits balance.
//!
//! Two implementations are provided:
//!
//! - [`SignedPositionDebt`] — debt on the same asset using negative amounts
//!   with paired creditor entries (the original model).
//! - [`SplitAssetDebt`] — debt on a separate `{asset}.d` signed asset,
//!   cleanly separating money from obligations.

mod signed_position;
mod split_asset;

pub use signed_position::SignedPositionDebt;
pub use split_asset::SplitAssetDebt;

use async_trait::async_trait;
use ledger_core::{AccountPath, Asset};

use crate::builder::TransactionBuilder;
use crate::error::Error;

/// Strategy for issuing and settling debt within ledger transactions.
///
/// The ledger enforces conservation (debits == credits per asset) but knows
/// nothing about debt. Implementations of this trait decide *how* to represent
/// obligations — which assets, which sign conventions — and inject the right
/// entries into the transaction builder. As long as both sides balance, the
/// ledger accepts the transaction.
#[async_trait]
pub trait DebtStrategy: Send + Sync {
    /// Add debt issuance entries to the transaction.
    ///
    /// `amount` is always positive — the strategy decides the sign convention.
    fn issue(
        &self,
        builder: TransactionBuilder,
        debtor: &AccountPath,
        creditor: &AccountPath,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error>;

    /// Add debt settlement entries to the transaction.
    ///
    /// `amount` is always positive — the strategy selects and consumes the
    /// appropriate debt tokens. The caller is responsible for adding the
    /// cash leg (debit payment source, credit cash account).
    async fn settle(
        &self,
        builder: TransactionBuilder,
        debtor: &AccountPath,
        creditor: &AccountPath,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error>;
}
