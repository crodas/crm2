//! High-level transaction builder with automatic token selection and
//! optional debt operations.

use std::collections::HashMap;
use std::sync::Arc;

use ledger_core::{
    AccountPath, Asset, Credit, LedgerError, SpendingToken, Storage, Transaction,
    TransactionBuilder as LowLevelBuilder,
};

use crate::debt::DebtStrategy;
use crate::error::Error;

/// A pending debit request: take `qty` of `asset` from `account`.
struct DebitRequest {
    account: String,
    asset_name: String,
    qty: String,
}

/// A pre-selected debit that bypasses token selection.
struct RawDebit {
    tx_id: String,
    entry_index: u32,
    owner: String,
    asset_name: String,
    qty: String,
}

/// High-level transaction builder that automatically selects unspent
/// tokens for debits and generates change credits.
///
/// Created via [`Ledger::transaction`](crate::Ledger::transaction).
///
/// # Token selection
///
/// For each debit, the builder queries storage for unspent tokens matching
/// the account and asset, selects the largest tokens first until the
/// requested amount is covered, and auto-generates a change credit back
/// to the source account if the selected tokens exceed the request.
///
/// # Debt operations
///
/// When a [`DebtStrategy`] is configured on the [`Ledger`](crate::Ledger),
/// the builder exposes [`create_debt`](Self::create_debt) and
/// [`settle_debt`](Self::settle_debt) as part of the fluent building flow.
pub struct TransactionBuilder {
    idempotency_key: String,
    storage: Arc<dyn Storage>,
    assets: HashMap<String, Asset>,
    debt_strategy: Option<Arc<dyn DebtStrategy>>,
    debits: Vec<DebitRequest>,
    raw_debits: Vec<RawDebit>,
    credits: Vec<Credit>,
}

impl TransactionBuilder {
    pub(crate) fn new(
        idempotency_key: String,
        storage: Arc<dyn Storage>,
        assets: HashMap<String, Asset>,
        debt_strategy: Option<Arc<dyn DebtStrategy>>,
    ) -> Self {
        Self {
            idempotency_key,
            storage,
            assets,
            debt_strategy,
            debits: Vec::new(),
            raw_debits: Vec::new(),
            credits: Vec::new(),
        }
    }

    /// Debit `qty` of `asset_name` from `account`.
    ///
    /// The builder will automatically select unspent tokens at build time.
    pub fn debit(
        mut self,
        account: impl Into<String>,
        asset_name: impl Into<String>,
        qty: impl Into<String>,
    ) -> Self {
        self.debits.push(DebitRequest {
            account: account.into(),
            asset_name: asset_name.into(),
            qty: qty.into(),
        });
        self
    }

    /// Credit `qty` of `asset_name` to `account`.
    pub fn credit(
        mut self,
        to: impl Into<String>,
        asset_name: impl Into<String>,
        qty: impl Into<String>,
    ) -> Self {
        self.credits.push(Credit {
            to: to.into(),
            asset_name: asset_name.into(),
            qty: qty.into(),
        });
        self
    }

    /// Add a pre-selected debit, bypassing automatic token selection.
    ///
    /// Use this when you have already performed token selection externally
    /// (e.g., debt settlement selects tokens with negative quantities).
    pub fn debit_raw(
        mut self,
        tx_id: impl Into<String>,
        entry_index: u32,
        owner: impl Into<String>,
        asset_name: impl Into<String>,
        qty: impl Into<String>,
    ) -> Self {
        self.raw_debits.push(RawDebit {
            tx_id: tx_id.into(),
            entry_index,
            owner: owner.into(),
            asset_name: asset_name.into(),
            qty: qty.into(),
        });
        self
    }

    // ── Debt operations ─────────────────────────────────────────────

    /// Issue debt: `debtor` owes `amount` of `asset` to `creditor`.
    ///
    /// Adds debt entries to the transaction using the configured
    /// [`DebtStrategy`]. The caller can chain additional entries
    /// (e.g., product debits for a credit sale) before building.
    ///
    /// Returns [`Error::NoDebtStrategy`] if no strategy is configured.
    pub fn create_debt(
        mut self,
        entity_id: i64,
        asset: &Asset,
        amount: i128,
    ) -> Result<Self, Error> {
        let strategy = Arc::clone(self.debt_strategy.as_ref().ok_or(Error::NoDebtStrategy)?);
        self = strategy.issue(self, &entity_id.to_string(), asset, amount)?;
        Ok(self)
    }

    /// Settle debt: reduce the entity's obligation by `amount`.
    ///
    /// Adds settlement entries to the transaction using the configured
    /// [`DebtStrategy`]. The caller is responsible for adding the cash
    /// leg (e.g., `.credit("@store/cash", "gs", "5000")`).
    ///
    /// Returns [`Error::NoDebtStrategy`] if no strategy is configured.
    pub async fn settle_debt(
        mut self,
        entity_id: i64,
        asset: &Asset,
        amount: i128,
    ) -> Result<Self, Error> {
        let strategy = Arc::clone(self.debt_strategy.as_ref().ok_or(Error::NoDebtStrategy)?);
        self = strategy
            .settle(self, &entity_id.to_string(), asset, amount)
            .await?;
        Ok(self)
    }

    // ── Build ─────────────────────────────────────────────────────────

    /// Build the transaction with automatic token selection.
    ///
    /// Queries storage for unspent tokens, selects them greedily (largest
    /// first), and generates change credits as needed. Then delegates to
    /// the low-level [`TransactionBuilder`] for validation.
    pub async fn build(self) -> Result<Transaction, Error> {
        let mut low = LowLevelBuilder::new(self.idempotency_key);

        // Process user-specified credits first.
        for credit in &self.credits {
            low = low.credit(&credit.to, &credit.asset_name, &credit.qty);
        }

        // Token selection for each debit.
        for req in &self.debits {
            let asset = self
                .assets
                .get(&req.asset_name)
                .ok_or_else(|| LedgerError::UnknownAsset(req.asset_name.clone()))?;

            let requested = asset
                .parse_qty(&req.qty)
                .map_err(|_| LedgerError::UnknownAsset(req.asset_name.clone()))?;

            let account = AccountPath::new(&req.account).map_err(|_| LedgerError::WorldAsOwner)?;

            let mut tokens = self
                .storage
                .unspent_by_account(&account, &req.asset_name)
                .await?;

            // Sort largest first for greedy selection.
            tokens.sort_by(|a, b| b.qty.cmp(&a.qty));

            let (selected, total) = select_tokens(&tokens, requested)?;

            if selected.is_empty() {
                return Err(Error::InsufficientBalance {
                    account: req.account.clone(),
                    asset: req.asset_name.clone(),
                    required: requested,
                    available: 0,
                });
            }

            // Add a DebitRef for each selected token.
            for token in &selected {
                low = low.debit(
                    &token.entry_ref.tx_id,
                    token.entry_ref.entry_index,
                    &req.account,
                    &req.asset_name,
                    asset.from_cents(token.qty),
                );
            }

            // Auto-generate change credit if needed.
            if total > requested {
                let change = total - requested;
                low = low.credit(&req.account, &req.asset_name, asset.from_cents(change));
            }
        }

        // Pass through pre-selected raw debits without token selection.
        for raw in &self.raw_debits {
            low = low.debit(
                &raw.tx_id,
                raw.entry_index,
                &raw.owner,
                &raw.asset_name,
                &raw.qty,
            );
        }

        let tx = low.build(&self.assets)?;
        Ok(tx)
    }
}

/// Greedily select tokens until `needed` is covered.
///
/// Tokens must be pre-sorted (largest first). Returns the selected tokens
/// and the total sum of their quantities.
fn select_tokens(
    tokens: &[SpendingToken],
    needed: i128,
) -> Result<(Vec<&SpendingToken>, i128), Error> {
    let mut selected = Vec::new();
    let mut sum: i128 = 0;

    for token in tokens {
        if sum >= needed {
            break;
        }
        selected.push(token);
        sum += token.qty;
    }

    if sum < needed {
        return Err(Error::InsufficientBalance {
            account: tokens
                .first()
                .map(|t| t.owner.as_str().to_string())
                .unwrap_or_default(),
            asset: tokens
                .first()
                .map(|t| t.asset_name.clone())
                .unwrap_or_default(),
            required: needed,
            available: sum,
        });
    }

    Ok((selected, sum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_core::{AssetKind, MemoryStorage};

    use crate::Ledger;

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage);
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

    /// Helper: issue tokens via the high-level builder.
    async fn issue(ledger: &Ledger, key: &str, account: &str, asset: &str, qty: &str) {
        let tx = ledger
            .transaction(key)
            .credit(account, asset, qty)
            .build()
            .await
            .expect("build issuance tx");
        ledger.commit(tx).await.expect("commit issuance tx");
    }

    #[tokio::test]
    async fn auto_token_selection_single_token() {
        let ledger = setup_ledger().await;

        issue(&ledger, "issue-001", "@store1/inventory", "brush", "10").await;

        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", "brush", "5")
            .credit("@customer1", "brush", "5")
            .build()
            .await
            .expect("build issuance tx");
        ledger.commit(tx).await.expect("commit issuance tx");

        assert_eq!(ledger.balance("@store1/inventory", "brush").await.unwrap(), 5);
        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 5);
    }

    #[tokio::test]
    async fn auto_token_selection_multiple_tokens() {
        let ledger = setup_ledger().await;

        // Issue 3 separate tokens: 2 + 3 + 4 = 9 brushes.
        for (i, qty) in [(1, "2"), (2, "3"), (3, "4")] {
            issue(
                &ledger,
                &format!("issue-{i}"),
                "@store1/inventory",
                "brush",
                qty,
            )
            .await;
        }

        // Request 6 brushes — should select 4 + 3 (largest first), change = 1.
        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", "brush", "6")
            .credit("@customer1", "brush", "6")
            .build()
            .await
            .expect("build issuance tx");
        ledger.commit(tx).await.expect("commit issuance tx");

        assert_eq!(ledger.balance("@store1/inventory", "brush").await.unwrap(), 3);
        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 6);
    }

    #[tokio::test]
    async fn exact_match_no_change() {
        let ledger = setup_ledger().await;

        issue(&ledger, "issue-001", "@store1/inventory", "brush", "5").await;

        // Debit exactly 5 — no change generated.
        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", "brush", "5")
            .credit("@customer1", "brush", "5")
            .build()
            .await
            .expect("build tx");

        assert_eq!(tx.debits.len(), 1);
        assert_eq!(tx.credits.len(), 1);

        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(ledger.balance("@store1/inventory", "brush").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn insufficient_balance_rejected() {
        let ledger = setup_ledger().await;

        issue(&ledger, "issue-001", "@store1/inventory", "brush", "3").await;

        let result = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", "brush", "10")
            .credit("@customer1", "brush", "10")
            .build()
            .await;

        assert!(matches!(result, Err(Error::InsufficientBalance { .. })));
    }

    #[tokio::test]
    async fn issuance_no_debits() {
        let ledger = setup_ledger().await;

        let tx = ledger
            .transaction("issue-001")
            .credit("@store1/inventory", "brush", "10")
            .build()
            .await
            .expect("build tx");

        assert!(tx.debits.is_empty());
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(ledger.balance("@store1/inventory", "brush").await.unwrap(), 10);
    }

    #[tokio::test]
    async fn multi_asset_debits() {
        let ledger = setup_ledger().await;

        issue(&ledger, "issue-brush", "@store1/inventory", "brush", "10").await;
        issue(&ledger, "issue-usd", "@store1/cash", "usd", "100.00").await;

        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", "brush", "3")
            .debit("@store1/cash", "usd", "25.00")
            .credit("@customer1", "brush", "3")
            .credit("@customer1", "usd", "25.00")
            .build()
            .await
            .expect("build issuance tx");
        ledger.commit(tx).await.expect("commit issuance tx");

        assert_eq!(ledger.balance("@store1/inventory", "brush").await.unwrap(), 7);
        assert_eq!(ledger.balance("@store1/cash", "usd").await.unwrap(), 7500);
        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 3);
        assert_eq!(ledger.balance("@customer1", "usd").await.unwrap(), 2500);
    }

    #[tokio::test]
    async fn credit_sale_via_high_level() {
        let ledger = setup_ledger().await;

        issue(&ledger, "issue-001", "@store1/inventory", "brush", "5").await;

        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", "brush", "2")
            .credit("@customer1", "brush", "2")
            .credit("@customer1", "usd", "-10.00")
            .credit("@store1/receivables", "usd", "10.00")
            .build()
            .await
            .expect("build issuance tx");
        ledger.commit(tx).await.expect("commit issuance tx");

        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 2);
        assert_eq!(ledger.balance("@customer1", "usd").await.unwrap(), -1000);
        assert_eq!(ledger.balance("@store1/inventory", "brush").await.unwrap(), 3);
    }
}
