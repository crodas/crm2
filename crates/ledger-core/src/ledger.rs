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

use crate::amount::Amount;
use crate::asset::Asset;
use crate::credit_token::{CreditEntryRef, CreditToken, CreditTokenStatus};
use crate::error::LedgerError;
use crate::storage::Storage;
use crate::transaction::{compute_tx_id, Transaction};

/// Aggregate a list of credit tokens into a map of asset name → net Amount.
fn aggregate_balances(tokens: Vec<CreditToken>) -> HashMap<Asset, Amount> {
    let mut map: HashMap<Asset, i128> = HashMap::new();
    for t in &tokens {
        *map.entry(t.amount.asset().clone()).or_insert(0) += t.amount.raw();
    }
    map.into_iter()
        .filter(|(_, raw)| *raw != 0)
        .map(|(asset, raw)| (asset.clone(), Amount::new_unchecked(asset, raw)))
        .collect()
}

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
/// let seven = brush.try_amount(7).unwrap();
/// let issue = TransactionBuilder::new("issue-001")
///     .credit("store1/inventory", &seven)
///     .credit("@world", &seven.negate())
///     .build()
///     .unwrap();
/// let tx_id = ledger.commit(issue).await.unwrap();
///
/// // Transfer 5 brushes to a customer, returning 2 as change.
/// let transfer = TransactionBuilder::new("sale-001")
///     .debit(&tx_id, 0, "store1/inventory", &brush.try_amount(7).unwrap())
///     .credit("customer1", &brush.try_amount(5).unwrap())
///     .credit("store1/inventory", &brush.try_amount(2).unwrap())
///     .build()
///     .unwrap();
/// ledger.commit(transfer).await.unwrap();
///
/// // Check balances.
/// let inv = ledger.balance("store1/inventory").await.unwrap();
/// assert_eq!(inv["brush"].raw(), 2);
/// let cust = ledger.balance("customer1").await.unwrap();
/// assert_eq!(cust["brush"].raw(), 5);
/// # });
/// ```
#[derive(Debug, Clone)]
pub struct Ledger {
    storage: Arc<dyn Storage>,
    /// Cached asset definitions, swapped atomically on registration.
    assets: Arc<ArcSwap<HashMap<String, Asset>>>,
}

impl Ledger {
    /// Create a new ledger backed by the given storage.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            assets: Arc::new(ArcSwap::from_pointee(HashMap::new())),
        }
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
    /// state: idempotency, credit token existence, single-spend, and field matching.
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
        let mut spent_refs: Vec<CreditEntryRef> = Vec::new();

        for debit in &tx.debits {
            let eref = CreditEntryRef {
                tx_id: debit.tx_id.clone(),
                entry_index: debit.entry_index,
            };

            let credit_to_spend = self
                .storage
                .get_credit_token(&eref)
                .await?
                .ok_or_else(|| LedgerError::DebitNotFound(eref.clone()))?;

            if credit_to_spend.status != CreditTokenStatus::Unspent {
                return Err(LedgerError::AlreadySpent(eref));
            }

            if debit.from.as_str() != credit_to_spend.owner.as_str() {
                return Err(LedgerError::DebitOwnerMismatch {
                    entry_ref: eref,
                    expected: credit_to_spend.owner.to_string(),
                    got: debit.from.to_string(),
                });
            }

            if debit.amount.asset_name() != credit_to_spend.amount.asset_name() {
                return Err(LedgerError::DebitAssetMismatch {
                    entry_ref: eref,
                    expected: credit_to_spend.amount.asset_name().to_string(),
                    got: debit.amount.asset_name().to_string(),
                });
            }

            if debit.amount.raw() != credit_to_spend.amount.raw() {
                return Err(LedgerError::DebitQtyMismatch {
                    entry_ref: eref,
                    expected: credit_to_spend.amount.raw(),
                    got: debit.amount.raw(),
                });
            }

            spent_refs.push(eref);
        }

        // Build new credit tokens from credits.
        let mut new_credits: Vec<CreditToken> = Vec::new();

        for (idx, credit) in tx.credits.iter().enumerate() {
            let eref = CreditEntryRef {
                tx_id: tx.tx_id.clone(),
                entry_index: idx as u32,
            };
            new_credits.push(CreditToken {
                entry_ref: eref,
                owner: credit.to.clone(),
                amount: credit.amount.clone(),
                status: CreditTokenStatus::Unspent,
            });
        }

        // Run the commit saga: mark spent → create credit tokens → insert tx.
        // On failure, completed steps are compensated in reverse order.
        crate::saga::run_commit(self.storage.clone(), spent_refs, new_credits, tx).await
    }

    /// Return the balance of a specific account across all assets.
    pub async fn balance(
        &self,
        account: &str,
    ) -> Result<HashMap<Asset, Amount>, LedgerError> {
        let tokens = self.storage.unspent_by_account(account, None).await?;
        Ok(aggregate_balances(tokens))
    }

    /// Return the aggregate balance of all accounts under a prefix.
    pub async fn balance_prefix(
        &self,
        prefix: &str,
    ) -> Result<HashMap<Asset, Amount>, LedgerError> {
        let tokens = self.storage.unspent_by_prefix(prefix, None).await?;
        Ok(aggregate_balances(tokens))
    }

    /// Return unspent tokens owned by the given account.
    ///
    /// - `Some(amount)` — only tokens matching the amount's asset; errors if
    ///   the available sum is less than `amount.raw()`.
    /// - `None` — all unspent tokens across all assets.
    pub async fn unspent_tokens(
        &self,
        account: &str,
        requested_amount: Option<&Amount>,
    ) -> Result<Vec<CreditToken>, LedgerError> {
        self.storage
            .unspent_by_account(account, requested_amount)
            .await
    }

    /// Return unspent tokens under a prefix.
    ///
    /// - `Some(amount)` — only tokens matching the amount's asset; errors if
    ///   the available sum is less than `amount.raw()`.
    /// - `None` — all unspent tokens across all assets.
    pub async fn unspent_tokens_prefix(
        &self,
        prefix: &str,
        requested_amount: Option<&Amount>,
    ) -> Result<Vec<CreditToken>, LedgerError> {
        self.storage
            .unspent_by_prefix(prefix, requested_amount)
            .await
    }

    /// Return aggregated balances grouped by account, then by asset name,
    /// for all unspent tokens under a prefix.
    pub async fn balances_by_prefix(
        &self,
        prefix: &str,
    ) -> Result<HashMap<String, HashMap<Asset, Amount>>, LedgerError> {
        self.storage.balances_by_prefix(prefix).await
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

    #[tokio::test]
    async fn issue_inventory() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let five_b = b.try_amount(5).unwrap();

        let tx = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build issuance");
        ledger.commit(tx).await.expect("commit issuance");

        assert_eq!(
            ledger
                .balance("store1/inventory")
                .await
                .expect("query balance")["brush"]
                .raw(),
            5
        );
    }

    #[tokio::test]
    async fn transfer_with_change() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let seven_b = b.try_amount(7).unwrap();

        let issue = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &seven_b)
            .credit("@world", &seven_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let transfer = TransactionBuilder::new("sale-001")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(7).unwrap())
            .credit("customer1/sale_1", &b.try_amount(5).unwrap())
            .credit("store1/inventory", &b.try_amount(2).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(transfer).await.expect("commit transfer");

        assert_eq!(
            ledger
                .balance("store1/inventory")
                .await
                .expect("store brush balance")["brush"]
                .raw(),
            2
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1")
                .await
                .expect("cust brush balance")["brush"]
                .raw(),
            5
        );
    }

    #[tokio::test]
    async fn credit_sale_with_debt() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let five_b = b.try_amount(5).unwrap();

        let issue = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let sale = TransactionBuilder::new("credit-sale-001")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(5).unwrap())
            .credit("customer1/sale_1", &b.try_amount(5).unwrap())
            .credit("customer1/sale_1", &u.try_amount(-1000).unwrap())
            .credit("store1/receivables/sale_1", &u.try_amount(1000).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(sale).await.expect("commit sale");

        assert_eq!(
            ledger
                .balance("customer1/sale_1")
                .await
                .expect("cust_sale brush balance")["brush"]
                .raw(),
            5
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1")
                .await
                .expect("cust_sale usd balance")["usd"]
                .raw(),
            -1000
        );
        assert_eq!(
            ledger
                .balance("store1/receivables/sale_1")
                .await
                .expect("store_recv usd balance")["usd"]
                .raw(),
            1000
        );
    }

    #[tokio::test]
    async fn full_credit_sale_lifecycle() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let five_b = b.try_amount(5).unwrap();

        let t1 = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("credit-sale-001")
            .debit(&t1_id, 0, "store1/inventory", &b.try_amount(5).unwrap())
            .credit("customer1/sale_1", &b.try_amount(5).unwrap())
            .credit("customer1/sale_1", &u.try_amount(-1000).unwrap())
            .credit("store1/receivables/sale_1", &u.try_amount(1000).unwrap())
            .build()
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let cash_1000 = u.try_amount(1000).unwrap();
        let t3 = TransactionBuilder::new("cash-in-001")
            .credit("customer1/cash", &cash_1000)
            .credit("@world", &cash_1000.negate())
            .build()
            .expect("build tx");
        let t3_id = ledger.commit(t3).await.expect("commit t3");

        let t4 = TransactionBuilder::new("partial-pay-001")
            .debit(&t3_id, 0, "customer1/cash", &u.try_amount(1000).unwrap())
            .debit(&t2_id, 1, "customer1/sale_1", &u.try_amount(-1000).unwrap())
            .debit(
                &t2_id,
                2,
                "store1/receivables/sale_1",
                &u.try_amount(1000).unwrap(),
            )
            .credit("store1/cash", &u.try_amount(600).unwrap())
            .credit("customer1/cash", &u.try_amount(400).unwrap())
            .credit("customer1/sale_1", &u.try_amount(-400).unwrap())
            .credit("store1/receivables/sale_1", &u.try_amount(400).unwrap())
            .build()
            .expect("build tx");
        let t4_id = ledger.commit(t4).await.expect("commit t4");

        assert_eq!(
            ledger
                .balance("store1/cash")
                .await
                .expect("store_cash usd balance")["usd"]
                .raw(),
            600
        );
        assert_eq!(
            ledger
                .balance("customer1/cash")
                .await
                .expect("cust_cash usd balance")["usd"]
                .raw(),
            400
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1")
                .await
                .expect("cust_sale usd balance")["usd"]
                .raw(),
            -400
        );
        assert_eq!(
            ledger
                .balance("store1/receivables/sale_1")
                .await
                .expect("store_recv usd balance")["usd"]
                .raw(),
            400
        );

        let t5 = TransactionBuilder::new("final-pay-001")
            .debit(&t4_id, 1, "customer1/cash", &u.try_amount(400).unwrap())
            .debit(&t4_id, 2, "customer1/sale_1", &u.try_amount(-400).unwrap())
            .debit(
                &t4_id,
                3,
                "store1/receivables/sale_1",
                &u.try_amount(400).unwrap(),
            )
            .credit("store1/cash", &u.try_amount(400).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(t5).await.expect("commit t5");

        assert_eq!(
            ledger
                .balance("store1/cash")
                .await
                .expect("store_cash usd balance")["usd"]
                .raw(),
            1000
        );
        assert_eq!(
            ledger
                .balance("customer1/cash")
                .await
                .expect("cust_cash usd balance")
                .get("usd")
                .map_or(0, |a| a.raw()),
            0
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1")
                .await
                .expect("cust_sale usd balance")
                .get("usd")
                .map_or(0, |a| a.raw()),
            0
        );
        assert_eq!(
            ledger
                .balance("store1/receivables/sale_1")
                .await
                .expect("store_recv usd balance")
                .get("usd")
                .map_or(0, |a| a.raw()),
            0
        );
        assert_eq!(
            ledger
                .balance("customer1/sale_1")
                .await
                .expect("cust_sale brush balance")["brush"]
                .raw(),
            5
        );
    }

    #[tokio::test]
    async fn prefix_query() {
        let ledger = setup_ledger().await;
        let u = usd(&ledger);
        let six_hundred = u.try_amount(600).unwrap();
        let four_hundred = u.try_amount(400).unwrap();

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

        assert_eq!(
            ledger
                .balance_prefix("store1")
                .await
                .expect("prefix usd prefix balance")["usd"]
                .raw(),
            1000
        );
    }

    #[tokio::test]
    async fn double_spend_rejected() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let five_b = b.try_amount(5).unwrap();

        let issue = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let spend1 = TransactionBuilder::new("spend-1")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(5).unwrap())
            .credit("customer1", &b.try_amount(5).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(spend1).await.expect("commit spend1");

        let spend2 = TransactionBuilder::new("spend-2")
            .debit(&issue_id, 0, "store1/inventory", &b.try_amount(5).unwrap())
            .credit("customer2", &b.try_amount(5).unwrap())
            .build()
            .expect("build tx");
        assert!(matches!(
            ledger.commit(spend2).await,
            Err(LedgerError::AlreadySpent(_))
        ));
    }

    /// Four credit tokens issued; one is spent by a prior transaction.
    /// A second transaction that tries to spend all four must fail with
    /// `AlreadySpent`, and the three remaining tokens must stay unspent.
    #[tokio::test]
    async fn race_one_of_four_inputs_already_spent() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let one_b = b.try_amount(1).unwrap();
        let neg_one_b = one_b.negate();

        // Issue 4 tokens of 1 brush each (indices 0..4 for positive credits).
        let issue = TransactionBuilder::new("issue-4")
            .credit("store1/inventory", &one_b)
            .credit("@world", &neg_one_b)
            .credit("store1/inventory", &one_b)
            .credit("@world", &neg_one_b)
            .credit("store1/inventory", &one_b)
            .credit("@world", &neg_one_b)
            .credit("store1/inventory", &one_b)
            .credit("@world", &neg_one_b)
            .build()
            .expect("build issue");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        // Spend credit token at index 4 (third positive credit) — simulates a
        // concurrent transaction that won the race.
        let race_winner = TransactionBuilder::new("race-winner")
            .debit(&issue_id, 4, "store1/inventory", &one_b)
            .credit("customer0", &one_b)
            .build()
            .expect("build race winner");
        ledger
            .commit(race_winner)
            .await
            .expect("commit race winner");

        // Now try to spend all 4 original tokens in one go.
        let race_loser = TransactionBuilder::new("race-loser")
            .debit(&issue_id, 0, "store1/inventory", &one_b)
            .debit(&issue_id, 2, "store1/inventory", &one_b)
            .debit(&issue_id, 4, "store1/inventory", &one_b)
            .debit(&issue_id, 6, "store1/inventory", &one_b)
            .credit("customer1", &b.try_amount(4).unwrap())
            .build()
            .expect("build race loser");

        assert!(matches!(
            ledger.commit(race_loser).await,
            Err(LedgerError::AlreadySpent(_))
        ));

        // The 3 unspent tokens must still be available.
        assert_eq!(
            ledger
                .balance("store1/inventory")
                .await
                .expect("balance after failed commit")["brush"]
                .raw(),
            3
        );
    }

    #[tokio::test]
    async fn conservation_enforced_at_build() {
        let b = brush(&setup_ledger().await);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "store1/inventory", &b.try_amount(5).unwrap())
            .credit("customer1", &b.try_amount(10).unwrap())
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
            .credit("customer1", &u.try_amount(-1000).unwrap())
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
        let five_b = b.try_amount(5).unwrap();
        let three_b = b.try_amount(3).unwrap();

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
        let ten_b = b.try_amount(10).unwrap();
        let five_k = u.try_amount(5000).unwrap();

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
                .balance("store1/inventory")
                .await
                .expect("inv brush balance")["brush"]
                .raw(),
            10
        );
        assert_eq!(
            ledger
                .balance("store1/cash")
                .await
                .expect("cash usd balance")["usd"]
                .raw(),
            5000
        );
    }

    #[tokio::test]
    async fn transfer_conserves_unsigned_asset() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let ten_b = b.try_amount(10).unwrap();

        let issue = TransactionBuilder::new("issue-001")
            .credit("a", &ten_b)
            .credit("@world", &ten_b.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let split = TransactionBuilder::new("split-001")
            .debit(&issue_id, 0, "a", &b.try_amount(10).unwrap())
            .credit("b", &b.try_amount(3).unwrap())
            .credit("c", &b.try_amount(5).unwrap())
            .credit("a", &b.try_amount(2).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(split).await.expect("commit split");

        assert_eq!(
            ledger.balance("a").await.expect("a brush balance")["brush"].raw(),
            2
        );
        assert_eq!(
            ledger.balance("b").await.expect("b brush balance")["brush"].raw(),
            3
        );
        assert_eq!(
            ledger.balance("c").await.expect("c brush balance")["brush"].raw(),
            5
        );
    }

    #[tokio::test]
    async fn transfer_credits_less_than_debits_rejected() {
        let b = brush(&setup_ledger().await);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake-tx", 0, "a", &b.try_amount(10).unwrap())
            .credit("b", &b.try_amount(7).unwrap())
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
            .debit("fake-tx", 0, "a", &b.try_amount(5).unwrap())
            .credit("b", &b.try_amount(8).unwrap())
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
        let ten_k = u.try_amount(10000).unwrap();

        let issue = TransactionBuilder::new("issue-001")
            .credit("a", &ten_k)
            .credit("@world", &ten_k.negate())
            .build()
            .expect("build tx");
        let issue_id = ledger.commit(issue).await.expect("commit issue");

        let transfer = TransactionBuilder::new("xfer-001")
            .debit(&issue_id, 0, "a", &u.try_amount(10000).unwrap())
            .credit("b", &u.try_amount(4000).unwrap())
            .credit("a", &u.try_amount(6000).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(transfer).await.expect("commit transfer");

        let a_usd = ledger.balance("a").await.expect("a usd balance")["usd"].raw();
        let b_usd = ledger.balance("b").await.expect("b usd balance")["usd"].raw();
        assert_eq!(a_usd + b_usd, 10000);
    }

    #[tokio::test]
    async fn debt_pair_nets_to_zero() {
        let ledger = setup_ledger().await;
        let u = usd(&ledger);

        let tx = TransactionBuilder::new("debt-001")
            .credit("debtor", &u.try_amount(-5000).unwrap())
            .credit("creditor", &u.try_amount(5000).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(
            ledger
                .balance("debtor")
                .await
                .expect("debtor usd balance")["usd"]
                .raw(),
            -5000
        );
        assert_eq!(
            ledger
                .balance("creditor")
                .await
                .expect("creditor usd balance")["usd"]
                .raw(),
            5000
        );
        let debtor_usd = ledger
            .balance("debtor")
            .await
            .expect("debtor usd balance")["usd"]
            .raw();
        let creditor_usd = ledger
            .balance("creditor")
            .await
            .expect("creditor usd balance")["usd"]
            .raw();
        assert_eq!(debtor_usd + creditor_usd, 0);
    }

    #[tokio::test]
    async fn settling_debt_zeroes_both_sides() {
        let ledger = setup_ledger().await;
        let u = usd(&ledger);

        let t1 = TransactionBuilder::new("debt-001")
            .credit("debtor", &u.try_amount(-5000).unwrap())
            .credit("creditor", &u.try_amount(5000).unwrap())
            .build()
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let five_k = u.try_amount(5000).unwrap();
        let t2 = TransactionBuilder::new("cash-in")
            .credit("debtor", &five_k)
            .credit("@world", &five_k.negate())
            .build()
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        let t3 = TransactionBuilder::new("settle-001")
            .debit(&t1_id, 0, "debtor", &u.try_amount(-5000).unwrap())
            .debit(&t2_id, 0, "debtor", &u.try_amount(5000).unwrap())
            .debit(&t1_id, 1, "creditor", &u.try_amount(5000).unwrap())
            .credit("creditor/cash", &u.try_amount(5000).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(t3).await.expect("commit t3");

        assert_eq!(
            ledger
                .balance("debtor")
                .await
                .expect("debtor usd balance")
                .get("usd")
                .map_or(0, |a| a.raw()),
            0
        );
        assert_eq!(
            ledger
                .balance_prefix("creditor")
                .await
                .expect("creditor_prefix usd prefix balance")["usd"]
                .raw(),
            5000
        );
    }

    #[tokio::test]
    async fn multi_asset_transfer_conserves_each_independently() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);
        let ten_b = b.try_amount(10).unwrap();
        let two_k = u.try_amount(2000).unwrap();

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
            .debit(&t1_id, 0, "a", &b.try_amount(10).unwrap())
            .debit(&t2_id, 0, "a", &u.try_amount(2000).unwrap())
            .credit("b", &b.try_amount(10).unwrap())
            .credit("b", &u.try_amount(2000).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(xfer).await.expect("commit xfer");

        assert_eq!(
            ledger.balance("a").await.expect("a brush balance").get("brush").map_or(0, |a| a.raw()),
            0
        );
        assert_eq!(
            ledger.balance("b").await.expect("b brush balance")["brush"].raw(),
            10
        );
        assert_eq!(
            ledger.balance("a").await.expect("a usd balance").get("usd").map_or(0, |a| a.raw()),
            0
        );
        assert_eq!(
            ledger.balance("b").await.expect("b usd balance")["usd"].raw(),
            2000
        );
    }

    #[tokio::test]
    async fn multi_asset_imbalance_rejected() {
        let ledger = setup_ledger().await;
        let b = brush(&ledger);
        let u = usd(&ledger);

        let result = TransactionBuilder::new("bad-001")
            .debit("fake1", 0, "a", &b.try_amount(10).unwrap())
            .debit("fake2", 0, "a", &u.try_amount(2000).unwrap())
            .credit("b", &b.try_amount(10).unwrap())
            .credit("b", &u.try_amount(1500).unwrap())
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
            .debit("fake", 0, "a", &b.try_amount(5).unwrap())
            .credit("b", &b.try_amount(5).unwrap())
            .credit("b", &u.try_amount(1000).unwrap())
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
        let five_b = b.try_amount(5).unwrap();
        let t1 = TransactionBuilder::new("issue-001")
            .credit("store1/inventory", &five_b)
            .credit("@world", &five_b.negate())
            .build()
            .expect("build tx");
        let t1_id = ledger.commit(t1).await.expect("commit t1");

        let t2 = TransactionBuilder::new("sale-001")
            .debit(&t1_id, 0, "store1/inventory", &b.try_amount(5).unwrap())
            .credit("customer1", &b.try_amount(2).unwrap())
            .credit("store1/inventory", &b.try_amount(3).unwrap())
            .credit("customer1", &u.try_amount(-1000).unwrap())
            .credit("store1/receivables", &u.try_amount(1000).unwrap())
            .build()
            .expect("build tx");
        let t2_id = ledger.commit(t2).await.expect("commit t2");

        assert_eq!(
            ledger
                .balance("customer1")
                .await
                .expect("cust usd balance")["usd"]
                .raw(),
            -1000
        );
        assert_eq!(
            ledger
                .balance("customer1")
                .await
                .expect("cust brush balance")["brush"]
                .raw(),
            2
        );

        let five_hundred = u.try_amount(500).unwrap();
        let t3 = TransactionBuilder::new("cash-in-001")
            .credit("customer1/cash", &five_hundred)
            .credit("@world", &five_hundred.negate())
            .build()
            .expect("build tx");
        let t3_id = ledger.commit(t3).await.expect("commit t3");

        let t4 = TransactionBuilder::new("pay-partial")
            .debit(&t3_id, 0, "customer1/cash", &u.try_amount(500).unwrap())
            .debit(&t2_id, 2, "customer1", &u.try_amount(-1000).unwrap())
            .debit(
                &t2_id,
                3,
                "store1/receivables",
                &u.try_amount(1000).unwrap(),
            )
            .credit("store1/cash", &u.try_amount(500).unwrap())
            .credit("customer1", &u.try_amount(-500).unwrap())
            .credit("store1/receivables", &u.try_amount(500).unwrap())
            .build()
            .expect("build tx");
        let t4_id = ledger.commit(t4).await.expect("commit t4");

        assert_eq!(
            ledger
                .balance("customer1")
                .await
                .expect("cust usd balance")["usd"]
                .raw(),
            -500
        );
        assert_eq!(
            ledger
                .balance_prefix("store1")
                .await
                .expect("store usd prefix balance")["usd"]
                .raw(),
            1000
        );

        let five_hundred_2 = u.try_amount(500).unwrap();
        let t5 = TransactionBuilder::new("cash-in-002")
            .credit("customer1/cash", &five_hundred_2)
            .credit("@world", &five_hundred_2.negate())
            .build()
            .expect("build tx");
        let t5_id = ledger.commit(t5).await.expect("commit t5");

        let t6 = TransactionBuilder::new("pay-final")
            .debit(&t5_id, 0, "customer1/cash", &u.try_amount(500).unwrap())
            .debit(&t4_id, 1, "customer1", &u.try_amount(-500).unwrap())
            .debit(&t4_id, 2, "store1/receivables", &u.try_amount(500).unwrap())
            .credit("store1/cash", &u.try_amount(500).unwrap())
            .build()
            .expect("build tx");
        ledger.commit(t6).await.expect("commit t6");

        assert_eq!(
            ledger
                .balance("customer1")
                .await
                .expect("cust usd balance")
                .get("usd")
                .map_or(0, |a| a.raw()),
            0
        );
        assert_eq!(
            ledger
                .balance("customer1")
                .await
                .expect("cust brush balance")["brush"]
                .raw(),
            2
        );
        assert_eq!(
            ledger
                .balance_prefix("store1")
                .await
                .expect("store usd prefix balance")["usd"]
                .raw(),
            1000
        );
    }
}
