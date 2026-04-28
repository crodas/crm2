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

use arc_swap::ArcSwap;

use crate::alias::AliasRegistry;
use crate::amount::Amount;
use crate::asset::Asset;
use crate::error::LedgerError;
use crate::storage::Storage;
use crate::token::{EntryRef, SpendingToken, TokenStatus};
use crate::transaction::{compute_tx_id, Transaction};

/// The append-only UTXO ledger engine.
///
/// Uses an [`Arc<dyn Storage>`] backend for persistence.
/// Asset definitions are cached via [`ArcSwap`] for lock-free reads.
///
/// # Example: issue inventory and transfer
///
/// ```
/// # use std::sync::Arc;
/// # tokio_test::block_on(async {
/// use ledger_core::*;
///
/// let storage = Arc::new(MemoryStorage::new());
/// let ledger = Ledger::new(storage);
/// let brush = Asset::new("brush", 0);
/// ledger.register_asset(brush.clone()).await.unwrap();
///
/// // Issue 7 brushes into store inventory.
/// let seven = brush.try_amount(7);
/// let issue = TransactionBuilder::new("issue-001")
///     .credit("store1/inventory", &seven)
///     .credit("@world", &seven.negate())
///     .build()
///     .unwrap();
/// let tx_id = ledger.commit(issue).await.unwrap();
///
/// // Transfer 5 brushes to a customer, returning 2 as change.
/// let transfer = TransactionBuilder::new("sale-001")
///     .debit(&tx_id, 0, "store1/inventory", &brush.try_amount(7))
///     .credit("customer1", &brush.try_amount(5))
///     .credit("store1/inventory", &brush.try_amount(2))
///     .build()
///     .unwrap();
/// ledger.commit(transfer).await.unwrap();
///
/// // Check balances.
/// assert_eq!(ledger.balance("store1/inventory", "brush").await.unwrap(), 2);
/// assert_eq!(ledger.balance("customer1", "brush").await.unwrap(), 5);
/// # });
/// ```
#[derive(Debug, Clone)]
pub struct Ledger {
    storage: Arc<dyn Storage>,
    /// Cached asset definitions, swapped atomically on registration.
    assets: Arc<ArcSwap<HashMap<String, Asset>>>,
    /// Alias rules — resolved before querying storage.
    aliases: Arc<AliasRegistry>,
}

impl Ledger {
    /// Create a new ledger backed by the given storage.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            assets: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            aliases: Arc::new(AliasRegistry::new()),
        }
    }

    /// Set the alias registry for this ledger.
    pub fn with_aliases(mut self, aliases: AliasRegistry) -> Self {
        self.aliases = Arc::new(aliases);
        self
    }

    /// Register an asset definition.
    ///
    /// Saves to storage and updates the local cache atomically.
    pub async fn register_asset(&self, asset: Asset) -> Result<(), LedgerError> {
        self.storage.register_asset(&asset).await?;
        let mut map = (**self.assets.load()).clone();
        map.insert(asset.name().to_string(), asset);
        self.assets.store(Arc::new(map));
        Ok(())
    }

    /// Return a snapshot of the cached asset definitions.
    pub fn assets(&self) -> Arc<HashMap<String, Asset>> {
        self.assets.load_full()
    }

    /// Look up a registered asset by name.
    pub fn asset(&self, name: &str) -> Option<Asset> {
        self.assets.load().get(name).cloned()
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
    pub async fn commit(&self, tx: Transaction) -> Result<String, LedgerError> {
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

            if debit.from.as_str() != token.owner.as_str() {
                return Err(LedgerError::DebitOwnerMismatch {
                    entry_ref: eref,
                    expected: token.owner.to_string(),
                    got: debit.from.to_string(),
                });
            }

            if debit.amount.asset_name() != token.amount.asset_name() {
                return Err(LedgerError::DebitAssetMismatch {
                    entry_ref: eref,
                    expected: token.amount.asset_name().to_string(),
                    got: debit.amount.asset_name().to_string(),
                });
            }

            if debit.amount.raw() != token.amount.raw() {
                return Err(LedgerError::DebitQtyMismatch {
                    entry_ref: eref,
                    expected: token.amount.raw(),
                    got: debit.amount.raw(),
                });
            }

            spent_refs.push(eref);
        }

        // Build new spending tokens from credits.
        let mut new_tokens: Vec<SpendingToken> = Vec::new();

        for (idx, credit) in tx.credits.iter().enumerate() {
            let eref = EntryRef {
                tx_id: tx.tx_id.clone(),
                entry_index: idx as u32,
            };
            new_tokens.push(SpendingToken {
                entry_ref: eref,
                owner: credit.to.clone(),
                amount: credit.amount.clone(),
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
    ///
    /// Alias rules are applied: if `account` matches an alias template,
    /// the canonical form is queried instead.
    pub async fn balance(&self, account: &str, asset_name: &str) -> Result<i128, LedgerError> {
        let resolved = self.aliases.resolve(account);
        let filter = Asset::new(asset_name, 0).max();
        let tokens = self
            .storage
            .unspent_by_account(&resolved, Some(&filter))
            .await?;
        Ok(tokens.iter().map(|t| t.amount.raw()).sum())
    }

    /// Return unspent tokens owned by the given account.
    ///
    /// Alias rules are applied before querying storage.
    pub async fn unspent_tokens(
        &self,
        account: &str,
        requested_amount: Option<&Amount>,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let resolved = self.aliases.resolve(account);
        self.storage
            .unspent_by_account(&resolved, requested_amount)
            .await
    }

    /// Return all distinct account names that have unspent tokens.
    pub async fn accounts(&self) -> Result<Vec<String>, LedgerError> {
        self.storage.accounts().await
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
    use crate::storage::MemoryStorage;
    use crate::TransactionBuilder;

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage);
        ledger
            .register_asset(Asset::new("brush", 0))
            .await
            .expect("register brush asset");
        ledger
            .register_asset(Asset::new("usd", 2))
            .await
            .expect("register usd asset");
        ledger
    }

    /// Helper: get the brush asset from the ledger.
    fn brush(ledger: &Ledger) -> Asset {
        ledger.asset("brush").expect("brush registered")
    }

    /// Helper: get the usd asset from the ledger.
    fn usd(ledger: &Ledger) -> Asset {
        ledger.asset("usd").expect("usd registered")
    }

    /// Helper: sum balances matching a prefix for a specific asset.
    async fn balance_search(ledger: &Ledger, prefix: &str, asset_name: &str) -> i128 {
        let accounts = ledger.accounts().await.unwrap();
        let mut sum = 0i128;
        for account in accounts {
            if account == prefix || account.starts_with(&format!("{prefix}/")) {
                sum += ledger.balance(&account, asset_name).await.unwrap();
            }
        }
        sum
    }

    #[tokio::test]
    async fn issue_inventory() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let five_b = b.try_amount(5);

        let tx = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build issuance");
        ledger.commit(tx).await.expect("commit issuance");

        assert_eq!(
            ledger
                .balance("store1/inventory", "brush")
                .await
                .expect("query balance"),
            5
        );
    }

    #[tokio::test]
    async fn transfer_with_change() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let seven_b = b.try_amount(7);

        let issue = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &seven_b)
            .credit("@world", &seven_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let transfer = TransactionBuilder::new("sale-001")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(7))
            .credit("customer1/sale_1", &b.try_amount(5))
            .credit("store1/inventory", &b.try_amount(2))
            .build()
            .expect("build tx");
        ledger.commit(transfer).await.expect("commit transfer");

        assert_eq!(
            ledger
                .balance("store1/inventory", "brush")
                .await
                .expect("store brush balance"),
            2
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1", "brush")
                .await
                .expect("cust brush balance"),
            5
        );
    }

    #[tokio::test]
    async fn credit_sale_with_debt() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let five_b = b.try_amount(5);

        let issue = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let sale = TransactionBuilder::new("credit-sale-001")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(5))
            .credit("customer1/sale_1", &b.try_amount(5))
            .credit("customer1/sale_1", &u.try_amount(-1000))
            .credit("store1/receivables/sale_1", &u.try_amount(1000))
            .build()
            .expect("build tx");
        ledger.commit(sale).await.expect("commit sale");

        assert_eq!(
            ledger
                .balance("customer1/sale_1", "brush")
                .await
                .expect("cust_sale brush balance"),
            5
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1", "usd")
                .await
                .expect("cust_sale usd balance"),
            -1000
        );
        assert_eq!(
            ledger
                .balance("store1/receivables/sale_1", "usd")
                .await
                .expect("store_recv usd balance"),
            1000
        );
    }

    #[tokio::test]
    async fn full_credit_sale_lifecycle() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let five_b = b.try_amount(5);

        let t1 = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("credit-sale-001")
            .debit(&t1_id, 0, "store1/inventory", &b.try_amount(5))
            .credit("customer1/sale_1", &b.try_amount(5))
            .credit("customer1/sale_1", &u.try_amount(-1000))
            .credit("store1/receivables/sale_1", &u.try_amount(1000))
            .build()
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let cash_1000 = u.try_amount(1000);
        let t3 = TransactionBuilder::new("cash-in-001")
            .credit("customer1/cash", &cash_1000)
            .credit("@world", &cash_1000.negate())
            .build()
            .expect("build tx");
        let t3_id = ledger.commit(t3).await.expect("commit t3");

        let t4 = TransactionBuilder::new("partial-pay-001")
            .debit(&t3_id, 0, "customer1/cash", &u.try_amount(1000))
            .debit(&t2_id, 1, "customer1/sale_1", &u.try_amount(-1000))
            .debit(&t2_id, 2, "store1/receivables/sale_1", &u.try_amount(1000))
            .credit("store1/cash", &u.try_amount(600))
            .credit("customer1/cash", &u.try_amount(400))
            .credit("customer1/sale_1", &u.try_amount(-400))
            .credit("store1/receivables/sale_1", &u.try_amount(400))
            .build()
            .expect("build tx");
        let t4_id = ledger.commit(t4).await.expect("commit t4");

        assert_eq!(
            ledger
                .balance("store1/cash", "usd")
                .await
                .expect("store_cash usd balance"),
            600
        );
        assert_eq!(
            ledger
                .balance("customer1/cash", "usd")
                .await
                .expect("cust_cash usd balance"),
            400
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1", "usd")
                .await
                .expect("cust_sale usd balance"),
            -400
        );
        assert_eq!(
            ledger
                .balance("store1/receivables/sale_1", "usd")
                .await
                .expect("store_recv usd balance"),
            400
        );

        let t5 = TransactionBuilder::new("final-pay-001")
            .debit(&t4_id, 1, "customer1/cash", &u.try_amount(400))
            .debit(&t4_id, 2, "customer1/sale_1", &u.try_amount(-400))
            .debit(&t4_id, 3, "store1/receivables/sale_1", &u.try_amount(400))
            .credit("store1/cash", &u.try_amount(400))
            .build()
            .expect("build tx");
        ledger.commit(t5).await.expect("commit t5");

        assert_eq!(
            ledger
                .balance("store1/cash", "usd")
                .await
                .expect("store_cash usd balance"),
            1000
        );
        assert_eq!(
            ledger
                .balance("customer1/cash", "usd")
                .await
                .expect("cust_cash usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1", "usd")
                .await
                .expect("cust_sale usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance("store1/receivables/sale_1", "usd")
                .await
                .expect("store_recv usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1", "brush")
                .await
                .expect("cust_sale brush balance"),
            5
        );
    }

    #[tokio::test]
    async fn prefix_query() {
        let ledger = setup_ledger().await;
        let u = usd(&ledger);
        let six_hundred = u.try_amount(600);
        let four_hundred = u.try_amount(400);

        let t1 = TransactionBuilder::new("k1")
            .credit("store1/cash", &six_hundred)
            .credit("@world", &six_hundred.negate())
            .build()
            .expect("build tx");
        ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("k2")
            .credit("store1/receivables/s1", &four_hundred)
            .credit("@world", &four_hundred.negate())
            .build()
            .expect("build tx");
        ledger.commit(t2).await.expect("commit t2");

        assert_eq!(balance_search(&ledger, "store1", "usd").await, 1000);
    }

    #[tokio::test]
    async fn double_spend_rejected() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let five_b = b.try_amount(5);

        let issue = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let spend1 = TransactionBuilder::new("spend-1")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(5))
            .credit("customer1", &b.try_amount(5))
            .build()
            .expect("build tx");
        ledger.commit(spend1).await.expect("commit spend1");

        let spend2 = TransactionBuilder::new("spend-2")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(5))
            .credit("customer2", &b.try_amount(5))
            .build()
            .expect("build tx");
        assert!(matches!(
            ledger.commit(spend2).await,
            Err(LedgerError::AlreadySpent(_))
        ));
    }

    #[tokio::test]
    async fn conservation_enforced_at_build() {
        let b = brush(&setup_ledger().await);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "store1/inventory", &b.try_amount(5))
            .credit("customer1", &b.try_amount(10))
            .build();
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn dangling_debt_rejected_at_build() {
        let u = usd(&setup_ledger().await);

        let result = TransactionBuilder::new("bad-001")
            .credit("customer1", &u.try_amount(-1000))
            .build();
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn duplicate_idempotency_key_rejected() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let five_b = b.try_amount(5);
        let three_b = b.try_amount(3);

        let tx1 = TransactionBuilder::new("same-key")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        ledger.commit(tx1).await.expect("commit tx1");

        let tx2 = TransactionBuilder::new("same-key")
            .credit("store1/inventory", &three_b)
            .credit("@world", &three_b.negate())
            .build()
            .expect("build tx");
        assert!(matches!(
            ledger.commit(tx2).await,
            Err(LedgerError::DuplicateIdempotencyKey(_))
        ));
    }

    // ── Transaction balance tests ──────────────────────────────────

    #[tokio::test]
    async fn issuance_creates_tokens_from_nothing() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let ten_b = b.try_amount(10);
        let five_k = u.try_amount(5000);

        let tx = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &ten_b)
            .credit("@world", &ten_b.negate())
            .credit("store1/cash", &five_k)
            .credit("@world", &five_k.negate())
            .build()
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(
            ledger
                .balance("store1/inventory", "brush")
                .await
                .expect("inv brush balance"),
            10
        );
        assert_eq!(
            ledger
                .balance("store1/cash", "usd")
                .await
                .expect("cash usd balance"),
            5000
        );
    }

    #[tokio::test]
    async fn transfer_conserves_unsigned_asset() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let ten_b = b.try_amount(10);

        let issue = TransactionBuilder::new("issue-001")
            .credit("a", &ten_b)
            .credit("@world", &ten_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let split = TransactionBuilder::new("split-001")
            .debit(&issue_id, 0, "a", &b.try_amount(10))
            .credit("b", &b.try_amount(3))
            .credit("c", &b.try_amount(5))
            .credit("a", &b.try_amount(2))
            .build()
            .expect("build tx");
        ledger.commit(split).await.expect("commit split");

        assert_eq!(
            ledger.balance("a", "brush").await.expect("a brush balance"),
            2
        );
        assert_eq!(
            ledger.balance("b", "brush").await.expect("b brush balance"),
            3
        );
        assert_eq!(
            ledger.balance("c", "brush").await.expect("c brush balance"),
            5
        );
    }

    #[tokio::test]
    async fn transfer_credits_less_than_debits_rejected() {
        let b = brush(&setup_ledger().await);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "a", &b.try_amount(10))
            .credit("b", &b.try_amount(7))
            .build();
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn transfer_credits_more_than_debits_rejected() {
        let b = brush(&setup_ledger().await);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "a", &b.try_amount(5))
            .credit("b", &b.try_amount(8))
            .build();
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn signed_asset_conservation_across_transfer() {
        let ledger = setup_ledger().await;
        let u = usd(&ledger);
        let ten_k = u.try_amount(10000);

        let issue = TransactionBuilder::new("issue-001")
            .credit("a", &ten_k)
            .credit("@world", &ten_k.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let transfer = TransactionBuilder::new("xfer-001")
            .debit(&issue_id, 0, "a", &u.try_amount(10000))
            .credit("b", &u.try_amount(4000))
            .credit("a", &u.try_amount(6000))
            .build()
            .expect("build tx");
        ledger.commit(transfer).await.expect("commit transfer");

        let sum = ledger.balance("a", "usd").await.expect("a usd balance")
            + ledger.balance("b", "usd").await.expect("b usd balance");
        assert_eq!(sum, 10000);
    }

    #[tokio::test]
    async fn debt_pair_nets_to_zero() {
        let ledger = setup_ledger().await;
        let u = usd(&ledger);

        let tx = TransactionBuilder::new("debt-001")
            .credit("debtor", &u.try_amount(-5000))
            .credit("creditor", &u.try_amount(5000))
            .build()
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(
            ledger
                .balance("debtor", "usd")
                .await
                .expect("debtor usd balance"),
            -5000
        );
        assert_eq!(
            ledger
                .balance("creditor", "usd")
                .await
                .expect("creditor usd balance"),
            5000
        );
        let sum = ledger
            .balance("debtor", "usd")
            .await
            .expect("debtor usd balance")
            + ledger
                .balance("creditor", "usd")
                .await
                .expect("creditor usd balance");
        assert_eq!(sum, 0);
    }

    #[tokio::test]
    async fn settling_debt_zeroes_both_sides() {
        let ledger = setup_ledger().await;
        let u = usd(&ledger);

        let t1 = TransactionBuilder::new("debt-001")
            .credit("debtor", &u.try_amount(-5000))
            .credit("creditor", &u.try_amount(5000))
            .build()
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let five_k = u.try_amount(5000);
        let t2 = TransactionBuilder::new("cash-in")
            .credit("debtor", &five_k)
            .credit("@world", &five_k.negate())
            .build()
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let t3 = TransactionBuilder::new("settle-001")
            .debit(&t1_id, 0, "debtor", &u.try_amount(-5000))
            .debit(&t2_id, 0, "debtor", &u.try_amount(5000))
            .debit(&t1_id, 1, "creditor", &u.try_amount(5000))
            .credit("creditor/cash", &u.try_amount(5000))
            .build()
            .expect("build tx");
        ledger.commit(t3).await.expect("commit t3");

        assert_eq!(
            ledger
                .balance("debtor", "usd")
                .await
                .expect("debtor usd balance"),
            0
        );
        assert_eq!(balance_search(&ledger, "creditor", "usd").await, 5000);
    }

    #[tokio::test]
    async fn multi_asset_transfer_conserves_each_independently() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let ten_b = b.try_amount(10);
        let two_k = u.try_amount(2000);

        let t1 = TransactionBuilder::new("issue-001")
            .credit("a", &ten_b)
            .credit("@world", &ten_b.negate())
            .build()
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("issue-002")
            .credit("a", &two_k)
            .credit("@world", &two_k.negate())
            .build()
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let xfer = TransactionBuilder::new("xfer-001")
            .debit(&t1_id, 0, "a", &b.try_amount(10))
            .debit(&t2_id, 0, "a", &u.try_amount(2000))
            .credit("b", &b.try_amount(10))
            .credit("b", &u.try_amount(2000))
            .build()
            .expect("build tx");
        ledger.commit(xfer).await.expect("commit xfer");

        assert_eq!(
            ledger.balance("a", "brush").await.expect("a brush balance"),
            0
        );
        assert_eq!(
            ledger.balance("b", "brush").await.expect("b brush balance"),
            10
        );
        assert_eq!(ledger.balance("a", "usd").await.expect("a usd balance"), 0);
        assert_eq!(
            ledger.balance("b", "usd").await.expect("b usd balance"),
            2000
        );
    }

    #[tokio::test]
    async fn multi_asset_imbalance_rejected() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake1", 0, "a", &b.try_amount(10))
            .debit("fake2", 0, "a", &u.try_amount(2000))
            .credit("b", &b.try_amount(10))
            .credit("b", &u.try_amount(1500))
            .build();
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
        let b = brush(&ledger);
        let u = usd(&ledger);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake", 0, "a", &b.try_amount(5))
            .credit("b", &b.try_amount(5))
            .credit("b", &u.try_amount(1000))
            .build();
        assert!(matches!(
            result,
            Err(LedgerError::ConservationViolated { .. })
        ));
    }

    #[tokio::test]
    async fn credit_sale_partial_and_full_settlement() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let five_b = b.try_amount(5);
        let t1 = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("sale-001")
            .debit(&t1_id, 0, "store1/inventory", &b.try_amount(5))
            .credit("customer1", &b.try_amount(2))
            .credit("store1/inventory", &b.try_amount(3))
            .credit("customer1", &u.try_amount(-1000))
            .credit("store1/receivables", &u.try_amount(1000))
            .build()
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        assert_eq!(
            ledger
                .balance("customer1", "usd")
                .await
                .expect("cust usd balance"),
            -1000
        );
        assert_eq!(
            ledger
                .balance("customer1", "brush")
                .await
                .expect("cust brush balance"),
            2
        );

        let five_hundred = u.try_amount(500);
        let t3 = TransactionBuilder::new("cash-in-001")
            .credit("customer1/cash", &five_hundred)
            .credit("@world", &five_hundred.negate())
            .build()
            .expect("build tx");
        let t3_id = ledger.commit(t3).await.expect("commit t3");

        let t4 = TransactionBuilder::new("pay-partial")
            .debit(&t3_id, 0, "customer1/cash", &u.try_amount(500))
            .debit(&t2_id, 2, "customer1", &u.try_amount(-1000))
            .debit(&t2_id, 3, "store1/receivables", &u.try_amount(1000))
            .credit("store1/cash", &u.try_amount(500))
            .credit("customer1", &u.try_amount(-500))
            .credit("store1/receivables", &u.try_amount(500))
            .build()
            .expect("build tx");
        let t4_id = ledger.commit(t4).await.expect("commit t4");

        assert_eq!(
            ledger
                .balance("customer1", "usd")
                .await
                .expect("cust usd balance"),
            -500
        );
        assert_eq!(balance_search(&ledger, "store1", "usd").await, 1000);

        let five_hundred_2 = u.try_amount(500);
        let t5 = TransactionBuilder::new("cash-in-002")
            .credit("customer1/cash", &five_hundred_2)
            .credit("@world", &five_hundred_2.negate())
            .build()
            .expect("build tx");
        let t5_id = ledger.commit(t5).await.expect("commit t5");

        let t6 = TransactionBuilder::new("pay-final")
            .debit(&t5_id, 0, "customer1/cash", &u.try_amount(500))
            .debit(&t4_id, 1, "customer1", &u.try_amount(-500))
            .debit(&t4_id, 2, "store1/receivables", &u.try_amount(500))
            .credit("store1/cash", &u.try_amount(500))
            .build()
            .expect("build tx");
        ledger.commit(t6).await.expect("commit t6");

        assert_eq!(
            ledger
                .balance("customer1", "usd")
                .await
                .expect("cust usd balance"),
            0
        );
        assert_eq!(
            ledger
                .balance("customer1", "brush")
                .await
                .expect("cust brush balance"),
            2
        );
        assert_eq!(balance_search(&ledger, "store1", "usd").await, 1000);
    }
}
