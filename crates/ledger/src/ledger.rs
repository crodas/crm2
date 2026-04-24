//! High-level ledger with automatic token selection and optional debt support.

use std::collections::HashMap;
use std::sync::Arc;

use ledger_core::{
    Asset, BalanceEntry, LedgerError, SpendingToken, Storage, Transaction,
    TransactionBuilder as LowLevelBuilder,
};

use crate::builder::TransactionBuilder;
use crate::issuance::IssuanceStrategy;
use crate::debt::DebtStrategy;

/// High-level ledger wrapping [`ledger_core::Ledger`] with automatic
/// token selection via [`TransactionBuilder`] and optional debt handling
/// via a pluggable [`DebtStrategy`].
///
/// # Debt support
///
/// The ledger has no built-in concept of debt. To use
/// [`TransactionBuilder::create_debt`] and [`TransactionBuilder::settle_debt`],
/// configure a strategy with [`with_debt_strategy`]:
///
/// ```ignore
/// let ledger = Ledger::new(storage)
///     .with_debt_strategy(SignedPositionDebt::new(
///         "customer/{id}/debt",
///         "store/receivables/{id}",
///     ));
/// ```
///
/// Without a strategy, debt methods on the builder return
/// [`Error::NoDebtStrategy`].
///
/// [`with_debt_strategy`]: Ledger::with_debt_strategy
pub struct Ledger {
    inner: ledger_core::Ledger,
    debt_strategy: Option<Arc<dyn DebtStrategy>>,
    issuance_strategy: Arc<dyn IssuanceStrategy>,
}

impl std::fmt::Debug for Ledger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ledger").finish_non_exhaustive()
    }
}

impl Ledger {
    /// Create a new ledger backed by the given storage.
    ///
    /// Uses a default issuance strategy with `@world` as the source.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        use crate::issuance::TemplateIssuanceStrategy;
        Self {
            inner: ledger_core::Ledger::new(storage),
            debt_strategy: None,
            issuance_strategy: Arc::new(TemplateIssuanceStrategy::new("@world")),
        }
    }

    /// Set the debt strategy for this ledger.
    pub fn with_debt_strategy(mut self, strategy: impl DebtStrategy + 'static) -> Self {
        self.debt_strategy = Some(Arc::new(strategy));
        self
    }

    /// Set the issuance strategy for this ledger.
    pub fn with_issuance_strategy(mut self, strategy: impl IssuanceStrategy + 'static) -> Self {
        self.issuance_strategy = Arc::new(strategy);
        self
    }

    // ── Transaction building ─────────────────────────────────────────

    /// Start building a high-level transaction with automatic token selection.
    pub fn transaction(&self, idempotency_key: impl Into<String>) -> TransactionBuilder {
        TransactionBuilder::new(
            idempotency_key.into(),
            Arc::clone(self.inner.storage()),
            self.debt_strategy.clone(),
            Arc::clone(&self.issuance_strategy),
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

    // ── Queries ──────────────────────────────────────────────────────

    /// Return the balance of a specific account for a given asset.
    pub async fn balance(&self, account: &str, asset_name: &str) -> Result<i128, LedgerError> {
        self.inner.balance(account, asset_name).await
    }

    /// Return the aggregate balance of all accounts under a prefix.
    pub async fn balance_prefix(
        &self,
        prefix: &str,
        asset_name: &str,
    ) -> Result<i128, LedgerError> {
        self.inner.balance_prefix(prefix, asset_name).await
    }

    /// Return all unspent tokens owned by the given account for a given asset.
    pub async fn unspent_tokens(
        &self,
        account: &str,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.inner.unspent_tokens(account, asset_name).await
    }

    /// Return all unspent tokens under a prefix for a given asset.
    pub async fn unspent_tokens_prefix(
        &self,
        prefix: &str,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.inner.unspent_tokens_prefix(prefix, asset_name).await
    }

    /// Return all unspent tokens under a prefix, across all assets.
    pub async fn unspent_all_by_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.inner.unspent_all_by_prefix(prefix).await
    }

    /// Return aggregated balances grouped by (account, asset) for all
    /// unspent tokens under a prefix.
    pub async fn balances_by_prefix(&self, prefix: &str) -> Result<Vec<BalanceEntry>, LedgerError> {
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
