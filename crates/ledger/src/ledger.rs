//! High-level ledger with automatic coin selection support.

use std::collections::HashMap;
use std::sync::Arc;

use ledger_core::{
    AccountPath, Asset, LedgerError, SpendingToken, Storage, Transaction,
    TransactionBuilder as LowLevelBuilder,
};

use crate::builder::TransactionBuilder;

/// High-level ledger wrapping [`ledger_core::Ledger`] with automatic
/// coin selection via [`TransactionBuilder`].
pub struct Ledger {
    inner: ledger_core::Ledger,
}

impl Ledger {
    /// Create a new ledger backed by the given storage.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            inner: ledger_core::Ledger::new(storage),
        }
    }

    /// Start building a high-level transaction with automatic coin selection.
    ///
    /// Debits only require `(account, asset, qty)` — the builder
    /// automatically selects unspent tokens and generates change.
    pub fn transaction(&self, idempotency_key: impl Into<String>) -> TransactionBuilder {
        TransactionBuilder::new(
            idempotency_key.into(),
            Arc::clone(self.inner.storage()),
            self.inner.assets().clone(),
        )
    }

    /// Start building a low-level transaction with explicit entry refs.
    pub fn transaction_low_level(&self, idempotency_key: impl Into<String>) -> LowLevelBuilder {
        LowLevelBuilder::new(idempotency_key)
    }

    /// Register an asset definition.
    pub async fn register_asset(&mut self, asset: Asset) -> Result<(), LedgerError> {
        self.inner.register_asset(asset).await
    }

    /// Return the cached asset definitions.
    pub fn assets(&self) -> &HashMap<String, Asset> {
        self.inner.assets()
    }

    /// Look up a registered asset by name.
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.inner.asset(name)
    }

    /// Commit a validated transaction to the ledger.
    pub async fn commit(&mut self, tx: Transaction) -> Result<String, LedgerError> {
        self.inner.commit(tx).await
    }

    /// Return the balance of a specific account for a given asset.
    pub async fn balance(
        &self,
        account: &AccountPath,
        asset_name: &str,
    ) -> Result<i128, LedgerError> {
        self.inner.balance(account, asset_name).await
    }

    /// Return the aggregate balance of all accounts under a prefix.
    pub async fn balance_prefix(
        &self,
        prefix: &AccountPath,
        asset_name: &str,
    ) -> Result<i128, LedgerError> {
        self.inner.balance_prefix(prefix, asset_name).await
    }

    /// Return all unspent tokens owned by the given account for a given asset.
    pub async fn unspent_tokens(
        &self,
        account: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.inner.unspent_tokens(account, asset_name).await
    }

    /// Return all unspent tokens under a prefix for a given asset.
    pub async fn unspent_tokens_prefix(
        &self,
        prefix: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.inner.unspent_tokens_prefix(prefix, asset_name).await
    }

    /// Return all committed transactions in append order.
    pub async fn transactions(&self) -> Result<Vec<Transaction>, LedgerError> {
        self.inner.transactions().await
    }

    /// Return the number of committed transactions.
    pub async fn tx_count(&self) -> Result<usize, LedgerError> {
        self.inner.tx_count().await
    }
}
