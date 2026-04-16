//! High-level transaction builder with automatic coin selection.

use std::collections::HashMap;
use std::sync::Arc;

use ledger_core::{
    AccountPath, Asset, Credit, LedgerError, SpendingToken, Storage, Transaction,
    TransactionBuilder as LowLevelBuilder,
};

use crate::error::Error;

/// A pending debit request: take `qty` of `asset` from `account`.
struct DebitRequest {
    account: String,
    asset_name: String,
    qty: String,
}

/// High-level transaction builder that automatically selects unspent
/// tokens for debits and generates change credits.
///
/// Created via [`Ledger::transaction`](crate::Ledger::transaction).
///
/// # Coin selection
///
/// For each debit, the builder queries storage for unspent tokens matching
/// the account and asset, selects the largest tokens first until the
/// requested amount is covered, and auto-generates a change credit back
/// to the source account if the selected tokens exceed the request.
pub struct TransactionBuilder {
    idempotency_key: String,
    storage: Arc<dyn Storage>,
    assets: HashMap<String, Asset>,
    debits: Vec<DebitRequest>,
    credits: Vec<Credit>,
}

impl TransactionBuilder {
    pub(crate) fn new(
        idempotency_key: String,
        storage: Arc<dyn Storage>,
        assets: HashMap<String, Asset>,
    ) -> Self {
        Self {
            idempotency_key,
            storage,
            assets,
            debits: Vec::new(),
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

    /// Build the transaction with automatic coin selection.
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

        // Coin selection for each debit.
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
                    asset.format_qty(token.qty),
                );
            }

            // Auto-generate change credit if needed.
            if total > requested {
                let change = total - requested;
                low = low.credit(&req.account, &req.asset_name, asset.format_qty(change));
            }
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
    async fn auto_coin_selection_single_token() {
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

        let store = AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        let cust = AccountPath::new("@customer1").expect("valid path: @customer1");
        assert_eq!(
            ledger
                .balance(&store, "brush")
                .await
                .expect("store brush balance"),
            5
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
    async fn auto_coin_selection_multiple_tokens() {
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

        let store = AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        let cust = AccountPath::new("@customer1").expect("valid path: @customer1");
        assert_eq!(
            ledger
                .balance(&store, "brush")
                .await
                .expect("store brush balance"),
            3
        ); // 2 remaining + 1 change
        assert_eq!(
            ledger
                .balance(&cust, "brush")
                .await
                .expect("cust brush balance"),
            6
        );
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

        let store = AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        assert_eq!(
            ledger
                .balance(&store, "brush")
                .await
                .expect("store brush balance"),
            0
        );
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

        let store = AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        assert_eq!(
            ledger
                .balance(&store, "brush")
                .await
                .expect("store brush balance"),
            10
        );
    }

    #[tokio::test]
    async fn multi_asset_debits() {
        let ledger = setup_ledger().await;

        issue(
            &ledger,
            "issue-brush",
            "@store1/inventory",
            "brush",
            "10",
        )
        .await;
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

        let store_inv =
            AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        let store_cash = AccountPath::new("@store1/cash").expect("valid path: @store1/cash");
        let cust = AccountPath::new("@customer1").expect("valid path: @customer1");
        assert_eq!(
            ledger
                .balance(&store_inv, "brush")
                .await
                .expect("store_inv brush balance"),
            7
        );
        assert_eq!(
            ledger
                .balance(&store_cash, "usd")
                .await
                .expect("store_cash usd balance"),
            7500
        );
        assert_eq!(
            ledger
                .balance(&cust, "brush")
                .await
                .expect("cust brush balance"),
            3
        );
        assert_eq!(
            ledger
                .balance(&cust, "usd")
                .await
                .expect("cust usd balance"),
            2500
        );
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

        let cust = AccountPath::new("@customer1").expect("valid path: @customer1");
        let store_inv =
            AccountPath::new("@store1/inventory").expect("valid path: @store1/inventory");
        assert_eq!(
            ledger
                .balance(&cust, "brush")
                .await
                .expect("cust brush balance"),
            2
        );
        assert_eq!(
            ledger
                .balance(&cust, "usd")
                .await
                .expect("cust usd balance"),
            -1000
        );
        assert_eq!(
            ledger
                .balance(&store_inv, "brush")
                .await
                .expect("store_inv brush balance"),
            3
        );
    }
}
