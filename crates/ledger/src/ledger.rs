//! High-level ledger with automatic coin selection and optional debt support.

use std::collections::HashMap;
use std::sync::Arc;

use ledger_core::{
    AccountPath, Asset, BalanceEntry, LedgerError, SpendingToken, Storage, Transaction,
    TransactionBuilder as LowLevelBuilder,
};

use crate::builder::TransactionBuilder;
use crate::debt::DebtStrategy;
use crate::error::Error;

/// High-level ledger wrapping [`ledger_core::Ledger`] with automatic
/// coin selection via [`TransactionBuilder`] and optional debt handling
/// via a pluggable [`DebtStrategy`].
///
/// # Debt support
///
/// The ledger has no built-in concept of debt. To use [`issue_debt`] and
/// [`settle_debt`], configure a strategy with [`with_debt_strategy`]:
///
/// ```ignore
/// let ledger = Ledger::new(storage)
///     .with_debt_strategy(SignedPositionDebt);
/// ```
///
/// Without a strategy, debt methods return [`Error::NoDebtStrategy`].
///
/// [`issue_debt`]: Ledger::issue_debt
/// [`settle_debt`]: Ledger::settle_debt
/// [`with_debt_strategy`]: Ledger::with_debt_strategy
pub struct Ledger {
    inner: ledger_core::Ledger,
    debt_strategy: Option<Box<dyn DebtStrategy>>,
}

impl Ledger {
    /// Create a new ledger backed by the given storage, with no debt strategy.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            inner: ledger_core::Ledger::new(storage),
            debt_strategy: None,
        }
    }

    /// Set the debt strategy for this ledger.
    ///
    /// Enables [`issue_debt`](Self::issue_debt) and
    /// [`settle_debt`](Self::settle_debt).
    pub fn with_debt_strategy(mut self, strategy: impl DebtStrategy + 'static) -> Self {
        self.debt_strategy = Some(Box::new(strategy));
        self
    }

    // ── Transaction building ─────────────────────────────────────────

    /// Start building a high-level transaction with automatic coin selection.
    ///
    /// Debits only require `(account, asset, qty)` — the builder
    /// automatically selects unspent tokens and generates change.
    pub fn transaction(&self, idempotency_key: impl Into<String>) -> TransactionBuilder {
        TransactionBuilder::new(
            idempotency_key.into(),
            Arc::clone(self.inner.storage()),
            (*self.inner.assets()).clone(),
        )
    }

    /// Start building a low-level transaction with explicit entry refs.
    pub fn transaction_low_level(&self, idempotency_key: impl Into<String>) -> LowLevelBuilder {
        LowLevelBuilder::new(idempotency_key)
    }

    // ── Assets ───────────────────────────────────────────────────────

    /// Register an asset definition.
    pub async fn register_asset(&self, asset: Asset) -> Result<(), LedgerError> {
        self.inner.register_asset(asset).await
    }

    /// Return a snapshot of the cached asset definitions.
    pub fn assets(&self) -> Arc<HashMap<String, Asset>> {
        self.inner.assets()
    }

    /// Look up a registered asset by name.
    pub fn asset(&self, name: &str) -> Option<Asset> {
        self.inner.asset(name)
    }

    // ── Commit ───────────────────────────────────────────────────────

    /// Commit a validated transaction to the ledger.
    pub async fn commit(&self, tx: Transaction) -> Result<String, LedgerError> {
        self.inner.commit(tx).await
    }

    // ── Debt ─────────────────────────────────────────────────────────

    /// Issue debt: `debtor` owes `amount` of `asset` to `creditor`.
    ///
    /// Adds debt entries to the given transaction builder using the
    /// configured [`DebtStrategy`]. The caller can add additional entries
    /// (e.g., product debits for a credit sale) before building.
    ///
    /// Returns [`Error::NoDebtStrategy`] if no strategy is configured.
    pub fn issue_debt(
        &self,
        builder: TransactionBuilder,
        debtor: &AccountPath,
        creditor: &AccountPath,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error> {
        let strategy = self.debt_strategy.as_ref().ok_or(Error::NoDebtStrategy)?;
        strategy.issue(builder, debtor, creditor, asset, amount)
    }

    /// Settle debt: reduce `debtor`'s obligation to `creditor` by `amount`.
    ///
    /// Adds settlement entries to the given transaction builder using the
    /// configured [`DebtStrategy`]. The caller is responsible for adding
    /// the cash leg (e.g., `.credit("@store/cash", "gs", "5000")`).
    ///
    /// Returns [`Error::NoDebtStrategy`] if no strategy is configured.
    pub async fn settle_debt(
        &self,
        builder: TransactionBuilder,
        debtor: &AccountPath,
        creditor: &AccountPath,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error> {
        let strategy = self.debt_strategy.as_ref().ok_or(Error::NoDebtStrategy)?;
        strategy
            .settle(builder, debtor, creditor, asset, amount)
            .await
    }

    // ── Queries ──────────────────────────────────────────────────────

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

    /// Return all unspent tokens under a prefix, across all assets.
    pub async fn unspent_all_by_prefix(
        &self,
        prefix: &AccountPath,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.inner.unspent_all_by_prefix(prefix).await
    }

    /// Return aggregated balances grouped by (account, asset) for all
    /// unspent tokens under a prefix.
    pub async fn balances_by_prefix(
        &self,
        prefix: &AccountPath,
    ) -> Result<Vec<BalanceEntry>, LedgerError> {
        self.inner.balances_by_prefix(prefix).await
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
