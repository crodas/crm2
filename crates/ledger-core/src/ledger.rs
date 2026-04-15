//! The core ledger engine backed by an async storage layer.
//!
//! The [`Ledger`] validates transactions against ledger state and delegates
//! persistence to an [`Arc<dyn Storage>`](crate::Storage) backend.
//!
//! Balance invariants (conservation, debt balancing, unsigned negativity) are
//! enforced at [`TransactionBuilder::build`] time — the `Transaction` type
//! is guaranteed balanced at construction.

use std::collections::HashMap;
use std::sync::Arc;

use crate::asset::Asset;
use crate::error::LedgerError;
use crate::storage::Storage;
use crate::token::{EntryRef, SpendingToken, TokenStatus};
use crate::transaction::{compute_tx_id, Transaction};
use crate::AccountPath;

/// The append-only UTXO ledger engine.
///
/// Uses an [`Arc<dyn Storage>`] backend for persistence. All mutating
/// operations are async.
///
/// # Example: issue inventory and transfer
///
/// ```
/// # use std::sync::Arc;
/// # tokio_test::block_on(async {
/// use ledger_core::*;
///
/// let storage = Arc::new(MemoryStorage::new());
/// let mut ledger = Ledger::new(storage);
/// ledger.register_asset(Asset::new("brush", 0, AssetKind::Unsigned)).await.unwrap();
///
/// // Issue 7 brushes from @world into store inventory.
/// let issue = TransactionBuilder::new("issue-001")
///     .credit("@store1/inventory", "brush", "7")
///     .build(ledger.assets())
///     .unwrap();
/// let tx_id = ledger.commit(issue).await.unwrap();
///
/// // Transfer 5 brushes to a customer, returning 2 as change.
/// let transfer = TransactionBuilder::new("sale-001")
///     .debit(&tx_id, 0, "@store1/inventory", "brush", "7")
///     .credit("@customer1", "brush", "5")
///     .credit("@store1/inventory", "brush", "2")
///     .build(ledger.assets())
///     .unwrap();
/// ledger.commit(transfer).await.unwrap();
///
/// // Check balances.
/// let store = AccountPath::new("@store1/inventory").unwrap();
/// assert_eq!(ledger.balance(&store, "brush").await.unwrap(), 2);
///
/// let cust = AccountPath::new("@customer1").unwrap();
/// assert_eq!(ledger.balance(&cust, "brush").await.unwrap(), 5);
/// # });
/// ```
pub struct Ledger {
    storage: Arc<dyn Storage>,
    /// Cached asset definitions (kept in sync with storage).
    assets: HashMap<String, Asset>,
}

impl Ledger {
    /// Create a new ledger backed by the given storage.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            assets: HashMap::new(),
        }
    }

    /// Register an asset definition.
    ///
    /// Saves to storage and updates the local cache.
    pub async fn register_asset(&mut self, asset: Asset) -> Result<(), LedgerError> {
        self.storage.save_asset(&asset).await?;
        self.assets.insert(asset.name().to_string(), asset);
        Ok(())
    }

    /// Return the cached asset definitions.
    pub fn assets(&self) -> &HashMap<String, Asset> {
        &self.assets
    }

    /// Look up a registered asset by name.
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.get(name)
    }

    /// Return a reference to the underlying storage backend.
    pub fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }

    /// Commit a validated transaction to the ledger.
    ///
    /// The transaction must have been built via [`TransactionBuilder::build`],
    /// which guarantees balance invariants. This method checks only ledger
    /// state: idempotency, token existence, single-spend, and field matching.
    ///
    /// Returns the transaction ID on success.
    pub async fn commit(&mut self, tx: Transaction) -> Result<String, LedgerError> {
        // Idempotency key uniqueness.
        if self
            .storage
            .has_idempotency_key(&tx.idempotency_key)
            .await?
        {
            return Err(LedgerError::DuplicateIdempotencyKey(
                tx.idempotency_key.clone(),
            ));
        }

        // Verify derived transaction ID.
        let expected_id = compute_tx_id(&tx.debits, &tx.credits, &tx.idempotency_key);
        if tx.tx_id != expected_id {
            return Err(LedgerError::TxIdMismatch {
                computed: expected_id,
                stored: tx.tx_id.clone(),
            });
        }

        // Validate debits against ledger state.
        let mut spent_refs: Vec<EntryRef> = Vec::new();

        for debit in &tx.debits {
            let eref = EntryRef {
                tx_id: debit.tx_id.clone(),
                entry_index: debit.entry_index,
            };

            let token = self
                .storage
                .get_token(&eref)
                .await?
                .ok_or_else(|| LedgerError::DebitNotFound(eref.clone()))?;

            if token.status != TokenStatus::Unspent {
                return Err(LedgerError::AlreadySpent(eref));
            }

            if debit.owner != token.owner.as_str() {
                return Err(LedgerError::DebitOwnerMismatch {
                    entry_ref: eref,
                    expected: token.owner.to_string(),
                    got: debit.owner.clone(),
                });
            }

            if debit.asset_name != token.asset_name {
                return Err(LedgerError::DebitAssetMismatch {
                    entry_ref: eref,
                    expected: token.asset_name.clone(),
                    got: debit.asset_name.clone(),
                });
            }

            let asset = self
                .assets
                .get(&debit.asset_name)
                .ok_or_else(|| LedgerError::UnknownAsset(debit.asset_name.clone()))?;
            let declared_qty = asset
                .parse_qty(&debit.qty)
                .map_err(|_| LedgerError::InvalidQty(debit.qty.clone()))?;

            if declared_qty != token.qty {
                return Err(LedgerError::DebitQtyMismatch {
                    entry_ref: eref,
                    expected: token.qty,
                    got: declared_qty,
                });
            }

            spent_refs.push(eref);
        }

        // Build new spending tokens from credits.
        let mut new_tokens: Vec<SpendingToken> = Vec::new();

        for (idx, credit) in tx.credits.iter().enumerate() {
            let asset = self
                .assets
                .get(&credit.asset_name)
                .ok_or_else(|| LedgerError::UnknownAsset(credit.asset_name.clone()))?;
            let qty = asset
                .parse_qty(&credit.qty)
                .map_err(|_| LedgerError::InvalidQty(credit.qty.clone()))?;
            let owner = AccountPath::new(&credit.to)
                .map_err(|_| LedgerError::InvalidAccount(credit.to.clone()))?;

            let eref = EntryRef {
                tx_id: tx.tx_id.clone(),
                entry_index: idx as u32,
            };
            new_tokens.push(SpendingToken {
                entry_ref: eref,
                owner,
                asset_name: credit.asset_name.clone(),
                qty,
                status: TokenStatus::Unspent,
            });
        }

        // Atomically persist everything.
        self.storage
            .commit_tx(&tx, &new_tokens, &spent_refs)
            .await?;

        Ok(tx.tx_id)
    }

    /// Return the balance of a specific account for a given asset.
    pub async fn balance(
        &self,
        account: &AccountPath,
        asset_name: &str,
    ) -> Result<i128, LedgerError> {
        let tokens = self.storage.unspent_by_account(account, asset_name).await?;
        Ok(tokens.iter().map(|t| t.qty).sum())
    }

    /// Return the aggregate balance of all accounts under a prefix.
    pub async fn balance_prefix(
        &self,
        prefix: &AccountPath,
        asset_name: &str,
    ) -> Result<i128, LedgerError> {
        let tokens = self.storage.unspent_by_prefix(prefix, asset_name).await?;
        Ok(tokens.iter().map(|t| t.qty).sum())
    }

    /// Return all unspent tokens owned by the given account for a given asset.
    pub async fn unspent_tokens(
        &self,
        account: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.storage.unspent_by_account(account, asset_name).await
    }

    /// Return all unspent tokens under a prefix for a given asset.
    pub async fn unspent_tokens_prefix(
        &self,
        prefix: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        self.storage.unspent_by_prefix(prefix, asset_name).await
    }

    /// Return all committed transactions in append order.
    pub async fn transactions(&self) -> Result<Vec<Transaction>, LedgerError> {
        self.storage.load_transactions().await
    }

    /// Return the number of committed transactions.
    pub async fn tx_count(&self) -> Result<usize, LedgerError> {
        self.storage.tx_count().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetKind;
    use crate::storage::MemoryStorage;
    use crate::TransactionBuilder;

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let mut ledger = Ledger::new(storage);
        ledger
            .register_asset(Asset::new("brush", 0, AssetKind::Unsigned))
            .await
            .expect("register brush asset");
        ledger
            .register_asset(Asset::new("usd", 2, AssetKind::Signed))
            .await
            .expect("register usd asset");
        ledger
    }

    #[tokio::test]
    async fn issue_inventory() {
        let mut ledger = setup_ledger().await;
        let tx = TransactionBuilder::new("issue-001")
            .credit("@store1/inventory", "brush", "5")
            .build(ledger.assets())
            .expect("build issuance");
        ledger.commit(tx).await.expect("commit issuance");

        let store = AccountPath::new("@store1/inventory").expect("valid path");
        assert_eq!(
            ledger
                .balance(&store, "brush")
                .await
                .expect("query balance"),
            5
        );
    }

    #[tokio::test]
    async fn transfer_with_change() {
        let mut ledger = setup_ledger().await;

        let issue = TransactionBuilder::new("issue-001")
            .credit("@store1/inventory", "brush", "7")
            .build(ledger.assets())
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let transfer = TransactionBuilder::new("sale-001")
            .debit(&issue_id, 0, "@store1/inventory", "brush", "7")
            .credit("@customer1/sale_1", "brush", "5")
            .credit("@store1/inventory", "brush", "2")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(transfer).await.expect("commit transfer");

        let store = AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        let cust = AccountPath::new("@customer1/sale_1").expect("valid path: @customer1/sale_1");
        assert_eq!(
            ledger
                .balance(&store, "brush")
                .await
                .expect("store brush balance"),
            2
        );
        assert_eq!(
            ledger
                .balance(&cust, "brush")
                .await
                .expect("cust brush balance"),
            5
        );
    }

    #[tokio::test]
    async fn credit_sale_with_debt() {
        let mut ledger = setup_ledger().await;

        let issue = TransactionBuilder::new("issue-001")
            .credit("@store1/inventory", "brush", "5")
            .build(ledger.assets())
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let sale = TransactionBuilder::new("credit-sale-001")
            .debit(&issue_id, 0, "@store1/inventory", "brush", "5")
            .credit("@customer1/sale_1", "brush", "5")
            .credit("@customer1/sale_1", "usd", "-10.00")
            .credit("@store1/receivables/sale_1", "usd", "10.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(sale).await.expect("commit sale");

        let cust_sale =
            AccountPath::new("@customer1/sale_1").expect("valid path: @customer1/sale_1");
        let store_recv = AccountPath::new("@store1/receivables/sale_1")
            .expect("valid path: @store1/receivables/sale_1");

        assert_eq!(
            ledger
                .balance(&cust_sale, "brush")
                .await
                .expect("cust_sale brush balance"),
            5
        );
        assert_eq!(
            ledger
                .balance(&cust_sale, "usd")
                .await
                .expect("cust_sale usd balance"),
            -1000
        );
        assert_eq!(
            ledger
                .balance(&store_recv, "usd")
                .await
                .expect("store_recv usd balance"),
            1000
        );
    }

    #[tokio::test]
    async fn full_credit_sale_lifecycle() {
        let mut ledger = setup_ledger().await;

        let t1 = TransactionBuilder::new("issue-001")
            .credit("@store1/inventory", "brush", "5")
            .build(ledger.assets())
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("credit-sale-001")
            .debit(&t1_id, 0, "@store1/inventory", "brush", "5")
            .credit("@customer1/sale_1", "brush", "5")
            .credit("@customer1/sale_1", "usd", "-10.00")
            .credit("@store1/receivables/sale_1", "usd", "10.00")
            .build(ledger.assets())
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let t3 = TransactionBuilder::new("cash-in-001")
            .credit("@customer1/cash", "usd", "10.00")
            .build(ledger.assets())
            .expect("build tx");
        let t3_id = ledger.commit(t3).await.expect("commit t3");

        let t4 = TransactionBuilder::new("partial-pay-001")
            .debit(&t3_id, 0, "@customer1/cash", "usd", "10.00")
            .debit(&t2_id, 1, "@customer1/sale_1", "usd", "-10.00")
            .debit(&t2_id, 2, "@store1/receivables/sale_1", "usd", "10.00")
            .credit("@store1/cash", "usd", "6.00")
            .credit("@customer1/cash", "usd", "4.00")
            .credit("@customer1/sale_1", "usd", "-4.00")
            .credit("@store1/receivables/sale_1", "usd", "4.00")
            .build(ledger.assets())
            .expect("build tx");
        let t4_id = ledger.commit(t4).await.expect("commit t4");

        let store_cash = AccountPath::new("@store1/cash").expect("valid path: @store1/cash");
        let cust_cash = AccountPath::new("@customer1/cash").expect("valid path: @customer1/cash");
        let cust_sale =
            AccountPath::new("@customer1/sale_1").expect("valid path: @customer1/sale_1");
        let store_recv = AccountPath::new("@store1/receivables/sale_1")
            .expect("valid path: @store1/receivables/sale_1");

        assert_eq!(
            ledger
                .balance(&store_cash, "usd")
                .await
                .expect("store_cash usd balance"),
            600
        );
        assert_eq!(
            ledger
                .balance(&cust_cash, "usd")
                .await
                .expect("cust_cash usd balance"),
            400
        );
        assert_eq!(
            ledger
                .balance(&cust_sale, "usd")
                .await
                .expect("cust_sale usd balance"),
            -400
        );
        assert_eq!(
            ledger
                .balance(&store_recv, "usd")
                .await
                .expect("store_recv usd balance"),
            400
        );

        let t5 = TransactionBuilder::new("final-pay-001")
            .debit(&t4_id, 1, "@customer1/cash", "usd", "4.00")
            .debit(&t4_id, 2, "@customer1/sale_1", "usd", "-4.00")
            .debit(&t4_id, 3, "@store1/receivables/sale_1", "usd", "4.00")
            .credit("@store1/cash", "usd", "4.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(t5).await.expect("commit t5");

        assert_eq!(
            ledger
                .balance(&store_cash, "usd")
                .await
                .expect("store_cash usd balance"),
            1000
        );
        assert_eq!(
            ledger
                .balance(&cust_cash, "usd")
                .await
                .expect("cust_cash usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance(&cust_sale, "usd")
                .await
                .expect("cust_sale usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance(&store_recv, "usd")
                .await
                .expect("store_recv usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance(&cust_sale, "brush")
                .await
                .expect("cust_sale brush balance"),
            5
        );
    }

    #[tokio::test]
    async fn prefix_query() {
        let mut ledger = setup_ledger().await;

        let t1 = TransactionBuilder::new("k1")
            .credit("@store1/cash", "usd", "6.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("k2")
            .credit("@store1/receivables/s1", "usd", "4.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(t2).await.expect("commit t2");

        let prefix = AccountPath::new("@store1").expect("valid path: @store1");
        assert_eq!(
            ledger
                .balance_prefix(&prefix, "usd")
                .await
                .expect("prefix usd prefix balance"),
            1000
        );
    }

    #[tokio::test]
    async fn double_spend_rejected() {
        let mut ledger = setup_ledger().await;

        let issue = TransactionBuilder::new("issue-001")
            .credit("@store1/inventory", "brush", "5")
            .build(ledger.assets())
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let spend1 = TransactionBuilder::new("spend-1")
            .debit(&issue_id, 0, "@store1/inventory", "brush", "5")
            .credit("@customer1", "brush", "5")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(spend1).await.expect("commit spend1");

        let spend2 = TransactionBuilder::new("spend-2")
            .debit(&issue_id, 0, "@store1/inventory", "brush", "5")
            .credit("@customer2", "brush", "5")
            .build(ledger.assets())
            .expect("build tx");
        assert!(matches!(
            ledger.commit(spend2).await,
            Err(LedgerError::AlreadySpent(_))
        ));
    }

    #[tokio::test]
    async fn conservation_enforced_at_build() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "@store1/inventory", "brush", "5")
            .credit("@customer1", "brush", "10")
            .build(ledger.assets());
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn dangling_debt_rejected_at_build() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .credit("@customer1", "usd", "-10.00")
            .build(ledger.assets());
        assert!(matches!(result, Err(LedgerError::DanglingDebt { .. })));
    }

    #[tokio::test]
    async fn negative_unsigned_rejected_at_build() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .credit("@store1/inventory", "brush", "-5")
            .build(ledger.assets());
        assert!(matches!(result, Err(LedgerError::NegativeUnsigned { .. })));
    }

    #[tokio::test]
    async fn duplicate_idempotency_key_rejected() {
        let mut ledger = setup_ledger().await;

        let tx1 = TransactionBuilder::new("same-key")
            .credit("@store1/inventory", "brush", "5")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(tx1).await.expect("commit tx1");

        let tx2 = TransactionBuilder::new("same-key")
            .credit("@store1/inventory", "brush", "3")
            .build(ledger.assets())
            .expect("build tx");
        assert!(matches!(
            ledger.commit(tx2).await,
            Err(LedgerError::DuplicateIdempotencyKey(_))
        ));
    }

    #[tokio::test]
    async fn world_as_owner_rejected_at_build() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .credit("@world", "brush", "5")
            .build(ledger.assets());
        assert!(matches!(result, Err(LedgerError::WorldAsOwner)));
    }

    // ── Transaction balance tests ──────────────────────────────────

    #[tokio::test]
    async fn issuance_creates_tokens_from_nothing() {
        let mut ledger = setup_ledger().await;

        let tx = TransactionBuilder::new("issue-001")
            .credit("@store1/inventory", "brush", "10")
            .credit("@store1/cash", "usd", "50.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        let inv = AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        let cash = AccountPath::new("@store1/cash").expect("valid path: @store1/cash");
        assert_eq!(
            ledger
                .balance(&inv, "brush")
                .await
                .expect("inv brush balance"),
            10
        );
        assert_eq!(
            ledger
                .balance(&cash, "usd")
                .await
                .expect("cash usd balance"),
            5000
        );
    }

    #[tokio::test]
    async fn transfer_conserves_unsigned_asset() {
        let mut ledger = setup_ledger().await;

        let issue = TransactionBuilder::new("issue-001")
            .credit("@a", "brush", "10")
            .build(ledger.assets())
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let split = TransactionBuilder::new("split-001")
            .debit(&issue_id, 0, "@a", "brush", "10")
            .credit("@b", "brush", "3")
            .credit("@c", "brush", "5")
            .credit("@a", "brush", "2")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(split).await.expect("commit split");

        let a = AccountPath::new("@a").expect("valid path: @a");
        let b = AccountPath::new("@b").expect("valid path: @b");
        let c = AccountPath::new("@c").expect("valid path: @c");
        assert_eq!(
            ledger.balance(&a, "brush").await.expect("a brush balance"),
            2
        );
        assert_eq!(
            ledger.balance(&b, "brush").await.expect("b brush balance"),
            3
        );
        assert_eq!(
            ledger.balance(&c, "brush").await.expect("c brush balance"),
            5
        );
    }

    #[tokio::test]
    async fn transfer_credits_less_than_debits_rejected() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "@a", "brush", "10")
            .credit("@b", "brush", "7")
            .build(ledger.assets());
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn transfer_credits_more_than_debits_rejected() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "@a", "brush", "5")
            .credit("@b", "brush", "8")
            .build(ledger.assets());
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn signed_asset_conservation_across_transfer() {
        let mut ledger = setup_ledger().await;

        let issue = TransactionBuilder::new("issue-001")
            .credit("@a", "usd", "100.00")
            .build(ledger.assets())
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let transfer = TransactionBuilder::new("xfer-001")
            .debit(&issue_id, 0, "@a", "usd", "100.00")
            .credit("@b", "usd", "40.00")
            .credit("@a", "usd", "60.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(transfer).await.expect("commit transfer");

        let a = AccountPath::new("@a").expect("valid path: @a");
        let b = AccountPath::new("@b").expect("valid path: @b");
        let sum = ledger.balance(&a, "usd").await.expect("a usd balance")
            + ledger.balance(&b, "usd").await.expect("b usd balance");
        assert_eq!(sum, 10000);
    }

    #[tokio::test]
    async fn debt_pair_nets_to_zero() {
        let mut ledger = setup_ledger().await;

        let tx = TransactionBuilder::new("debt-001")
            .credit("@debtor", "usd", "-50.00")
            .credit("@creditor", "usd", "50.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        let debtor = AccountPath::new("@debtor").expect("valid path: @debtor");
        let creditor = AccountPath::new("@creditor").expect("valid path: @creditor");
        assert_eq!(
            ledger
                .balance(&debtor, "usd")
                .await
                .expect("debtor usd balance"),
            -5000
        );
        assert_eq!(
            ledger
                .balance(&creditor, "usd")
                .await
                .expect("creditor usd balance"),
            5000
        );
        let sum = ledger
            .balance(&debtor, "usd")
            .await
            .expect("debtor usd balance")
            + ledger
                .balance(&creditor, "usd")
                .await
                .expect("creditor usd balance");
        assert_eq!(sum, 0);
    }

    #[tokio::test]
    async fn settling_debt_zeroes_both_sides() {
        let mut ledger = setup_ledger().await;

        let t1 = TransactionBuilder::new("debt-001")
            .credit("@debtor", "usd", "-50.00")
            .credit("@creditor", "usd", "50.00")
            .build(ledger.assets())
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("cash-in")
            .credit("@debtor", "usd", "50.00")
            .build(ledger.assets())
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let t3 = TransactionBuilder::new("settle-001")
            .debit(&t1_id, 0, "@debtor", "usd", "-50.00")
            .debit(&t2_id, 0, "@debtor", "usd", "50.00")
            .debit(&t1_id, 1, "@creditor", "usd", "50.00")
            .credit("@creditor/cash", "usd", "50.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(t3).await.expect("commit t3");

        let debtor = AccountPath::new("@debtor").expect("valid path: @debtor");
        let creditor_prefix = AccountPath::new("@creditor").expect("valid path: @creditor");
        assert_eq!(
            ledger
                .balance(&debtor, "usd")
                .await
                .expect("debtor usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance_prefix(&creditor_prefix, "usd")
                .await
                .expect("creditor_prefix usd prefix balance"),
            5000
        );
    }

    #[tokio::test]
    async fn multi_asset_transfer_conserves_each_independently() {
        let mut ledger = setup_ledger().await;

        let t1 = TransactionBuilder::new("issue-001")
            .credit("@a", "brush", "10")
            .build(ledger.assets())
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("issue-002")
            .credit("@a", "usd", "20.00")
            .build(ledger.assets())
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let xfer = TransactionBuilder::new("xfer-001")
            .debit(&t1_id, 0, "@a", "brush", "10")
            .debit(&t2_id, 0, "@a", "usd", "20.00")
            .credit("@b", "brush", "10")
            .credit("@b", "usd", "20.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(xfer).await.expect("commit xfer");

        let a = AccountPath::new("@a").expect("valid path: @a");
        let b = AccountPath::new("@b").expect("valid path: @b");
        assert_eq!(
            ledger.balance(&a, "brush").await.expect("a brush balance"),
            0
        );
        assert_eq!(
            ledger.balance(&b, "brush").await.expect("b brush balance"),
            10
        );
        assert_eq!(ledger.balance(&a, "usd").await.expect("a usd balance"), 0);
        assert_eq!(
            ledger.balance(&b, "usd").await.expect("b usd balance"),
            2000
        );
    }

    #[tokio::test]
    async fn multi_asset_imbalance_rejected() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .debit("fake1", 0, "@a", "brush", "10")
            .debit("fake2", 0, "@a", "usd", "20.00")
            .credit("@b", "brush", "10")
            .credit("@b", "usd", "15.00")
            .build(ledger.assets());
        let err = result.expect_err("should fail with conservation error");
        match err {
            LedgerError::ConservationViolated {
                asset,
                debit_sum,
                credit_sum,
            } => {
                assert_eq!(asset, "usd");
                assert_eq!(debit_sum, 2000);
                assert_eq!(credit_sum, 1500);
            }
            other => panic!("expected ConservationViolated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn credit_only_asset_not_in_debits_rejected() {
        let ledger = setup_ledger().await;

        let result = TransactionBuilder::new("bad-001")
            .debit("fake", 0, "@a", "brush", "5")
            .credit("@b", "brush", "5")
            .credit("@b", "usd", "10.00")
            .build(ledger.assets());
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn credit_sale_partial_and_full_settlement() {
        let mut ledger = setup_ledger().await;
        let cust = AccountPath::new("@customer1").expect("valid path: @customer1");
        let store = AccountPath::new("@store1").expect("valid path: @store1");

        let t1 = TransactionBuilder::new("issue-001")
            .credit("@store1/inventory", "brush", "5")
            .build(ledger.assets())
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("sale-001")
            .debit(&t1_id, 0, "@store1/inventory", "brush", "5")
            .credit("@customer1", "brush", "2")
            .credit("@store1/inventory", "brush", "3")
            .credit("@customer1", "usd", "-10.00")
            .credit("@store1/receivables", "usd", "10.00")
            .build(ledger.assets())
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        assert_eq!(
            ledger
                .balance(&cust, "usd")
                .await
                .expect("cust usd balance"),
            -1000
        );
        assert_eq!(
            ledger
                .balance(&cust, "brush")
                .await
                .expect("cust brush balance"),
            2
        );

        let t3 = TransactionBuilder::new("cash-in-001")
            .credit("@customer1/cash", "usd", "5.00")
            .build(ledger.assets())
            .expect("build tx");
        let t3_id = ledger.commit(t3).await.expect("commit t3");

        let t4 = TransactionBuilder::new("pay-partial")
            .debit(&t3_id, 0, "@customer1/cash", "usd", "5.00")
            .debit(&t2_id, 2, "@customer1", "usd", "-10.00")
            .debit(&t2_id, 3, "@store1/receivables", "usd", "10.00")
            .credit("@store1/cash", "usd", "5.00")
            .credit("@customer1", "usd", "-5.00")
            .credit("@store1/receivables", "usd", "5.00")
            .build(ledger.assets())
            .expect("build tx");
        let t4_id = ledger.commit(t4).await.expect("commit t4");

        assert_eq!(
            ledger
                .balance(&cust, "usd")
                .await
                .expect("cust usd balance"),
            -500
        );
        assert_eq!(
            ledger
                .balance_prefix(&store, "usd")
                .await
                .expect("store usd prefix balance"),
            1000
        );

        let t5 = TransactionBuilder::new("cash-in-002")
            .credit("@customer1/cash", "usd", "5.00")
            .build(ledger.assets())
            .expect("build tx");
        let t5_id = ledger.commit(t5).await.expect("commit t5");

        let t6 = TransactionBuilder::new("pay-final")
            .debit(&t5_id, 0, "@customer1/cash", "usd", "5.00")
            .debit(&t4_id, 1, "@customer1", "usd", "-5.00")
            .debit(&t4_id, 2, "@store1/receivables", "usd", "5.00")
            .credit("@store1/cash", "usd", "5.00")
            .build(ledger.assets())
            .expect("build tx");
        ledger.commit(t6).await.expect("commit t6");

        assert_eq!(
            ledger
                .balance(&cust, "usd")
                .await
                .expect("cust usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance(&cust, "brush")
                .await
                .expect("cust brush balance"),
            2
        );
        assert_eq!(
            ledger
                .balance_prefix(&store, "usd")
                .await
                .expect("store usd prefix balance"),
            1000
        );
    }
}
