//! High-level transaction builder with automatic token selection and
//! optional debt operations.

use std::sync::Arc;

use ledger_core::{
    AccountPath, Amount, LedgerError, SpendingToken, Storage, Transaction,
    TransactionBuilder as LowLevelBuilder,
};

use crate::debt::DebtStrategy;
use crate::error::Error;

/// A pending debit request: take `amount` from `account`.
struct DebitRequest {
    account: String,
    amount: Amount,
}

/// A pre-selected debit that bypasses token selection.
struct RawDebit {
    tx_id: String,
    entry_index: u32,
    from: String,
    amount: Amount,
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
    debt_strategy: Option<Arc<dyn DebtStrategy>>,
    debits: Vec<DebitRequest>,
    raw_debits: Vec<RawDebit>,
    credits: Vec<(String, Amount)>,
}

impl TransactionBuilder {
    pub(crate) fn new(
        idempotency_key: String,
        storage: Arc<dyn Storage>,
        debt_strategy: Option<Arc<dyn DebtStrategy>>,
    ) -> Self {
        Self {
            idempotency_key,
            storage,
            debt_strategy,
            debits: Vec::new(),
            raw_debits: Vec::new(),
            credits: Vec::new(),
        }
    }

    /// Debit `amount` from `account`.
    ///
    /// The builder will automatically select unspent tokens at build time.
    pub fn debit(mut self, from: impl Into<String>, amount: &Amount) -> Self {
        self.debits.push(DebitRequest {
            account: from.into(),
            amount: amount.clone(),
        });
        self
    }

    /// Credit `amount` to `account`.
    pub fn credit(mut self, to: impl Into<String>, amount: &Amount) -> Self {
        self.credits.push((to.into(), amount.clone()));
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
        from: impl Into<String>,
        amount: &Amount,
    ) -> Self {
        self.raw_debits.push(RawDebit {
            tx_id: tx_id.into(),
            entry_index,
            from: from.into(),
            amount: amount.clone(),
        });
        self
    }

    // ── Debt operations ─────────────────────────────────────────────

    /// Issue debt for `entity_id` using the configured [`DebtStrategy`].
    ///
    /// Returns [`Error::NoDebtStrategy`] if no strategy is configured.
    pub fn create_debt(mut self, entity_id: i64, amount: &Amount) -> Result<Self, Error> {
        let strategy = Arc::clone(self.debt_strategy.as_ref().ok_or(Error::NoDebtStrategy)?);
        self = strategy.issue(self, &entity_id.to_string(), amount)?;
        Ok(self)
    }

    /// Settle debt for `entity_id` using the configured [`DebtStrategy`].
    ///
    /// The caller is responsible for adding the cash leg.
    ///
    /// Returns [`Error::NoDebtStrategy`] if no strategy is configured.
    pub async fn settle_debt(mut self, entity_id: i64, amount: &Amount) -> Result<Self, Error> {
        let strategy = Arc::clone(self.debt_strategy.as_ref().ok_or(Error::NoDebtStrategy)?);
        self = strategy
            .settle(self, &entity_id.to_string(), amount)
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
        for (to, amount) in &self.credits {
            low = low.credit(to, amount)?;
        }

        // Token selection for each debit.
        for req in &self.debits {
            let account = AccountPath::new(&req.account).map_err(|_| LedgerError::WorldAsOwner)?;

            let mut tokens = self
                .storage
                .unspent_by_account(&account, req.amount.asset_name())
                .await?;

            // Sort largest first for greedy selection.
            tokens.sort_by(|a, b| b.amount.raw().cmp(&a.amount.raw()));

            let (selected, total) = select_tokens(&tokens, req.amount.raw())?;

            if selected.is_empty() {
                return Err(Error::InsufficientBalance {
                    account: req.account.clone(),
                    asset: req.amount.asset_name().to_string(),
                    required: req.amount.raw(),
                    available: 0,
                });
            }

            // Add a DebitRef for each selected token.
            for token in &selected {
                low = low.debit(
                    &token.entry_ref.tx_id,
                    token.entry_ref.entry_index,
                    &req.account,
                    &token.amount,
                )?;
            }

            // Auto-generate change credit if needed.
            if total > req.amount.raw() {
                let change = total - req.amount.raw();
                let change_amount = req.amount.asset().amount_unchecked(change);
                low = low.credit(&req.account, &change_amount)?;
            }
        }

        // Pass through pre-selected raw debits without token selection.
        for raw in &self.raw_debits {
            low = low.debit(&raw.tx_id, raw.entry_index, &raw.from, &raw.amount)?;
        }

        let tx = low.build()?;
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
        sum += token.amount.raw();
    }

    if sum < needed {
        return Err(Error::InsufficientBalance {
            account: tokens
                .first()
                .map(|t| t.owner.as_str().to_string())
                .unwrap_or_default(),
            asset: tokens
                .first()
                .map(|t| t.amount.asset_name().to_string())
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
    use ledger_core::{Asset, AssetKind, MemoryStorage};

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
    async fn issue(ledger: &Ledger, key: &str, account: &str, amount: &Amount) {
        let tx = ledger
            .transaction(key)
            .credit(account, amount)
            .build()
            .await
            .expect("build issuance tx");
        ledger.commit(tx).await.expect("commit issuance tx");
    }

    #[tokio::test]
    async fn auto_token_selection_single_token() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();
        let b10 = brush.try_amount(10).unwrap();
        let b5 = brush.try_amount(5).unwrap();

        issue(&ledger, "issue-001", "@store1/inventory", &b10).await;

        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", &b5)
            .credit("@customer1", &b5)
            .build()
            .await
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(
            ledger.balance("@store1/inventory", "brush").await.unwrap(),
            5
        );
        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 5);
    }

    #[tokio::test]
    async fn auto_token_selection_multiple_tokens() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();

        for (i, qty) in [(1, 2), (2, 3), (3, 4)] {
            let amt = brush.try_amount(qty).unwrap();
            issue(&ledger, &format!("issue-{i}"), "@store1/inventory", &amt).await;
        }

        let b6 = brush.try_amount(6).unwrap();
        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", &b6)
            .credit("@customer1", &b6)
            .build()
            .await
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(
            ledger.balance("@store1/inventory", "brush").await.unwrap(),
            3
        );
        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 6);
    }

    #[tokio::test]
    async fn exact_match_no_change() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();
        let b5 = brush.try_amount(5).unwrap();

        issue(&ledger, "issue-001", "@store1/inventory", &b5).await;

        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", &b5)
            .credit("@customer1", &b5)
            .build()
            .await
            .expect("build tx");

        assert_eq!(tx.debits.len(), 1);
        assert_eq!(tx.credits.len(), 1);

        ledger.commit(tx).await.expect("commit tx");
        assert_eq!(
            ledger.balance("@store1/inventory", "brush").await.unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn insufficient_balance_rejected() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();
        let b3 = brush.try_amount(3).unwrap();
        let b10 = brush.try_amount(10).unwrap();

        issue(&ledger, "issue-001", "@store1/inventory", &b3).await;

        let result = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", &b10)
            .credit("@customer1", &b10)
            .build()
            .await;

        assert!(matches!(result, Err(Error::InsufficientBalance { .. })));
    }

    #[tokio::test]
    async fn issuance_no_debits() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();
        let b10 = brush.try_amount(10).unwrap();

        let tx = ledger
            .transaction("issue-001")
            .credit("@store1/inventory", &b10)
            .build()
            .await
            .expect("build tx");

        assert!(tx.debits.is_empty());
        ledger.commit(tx).await.expect("commit tx");
        assert_eq!(
            ledger.balance("@store1/inventory", "brush").await.unwrap(),
            10
        );
    }

    #[tokio::test]
    async fn multi_asset_debits() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();
        let usd = ledger.asset("usd").unwrap();

        let b10 = brush.try_amount(10).unwrap();
        let b3 = brush.try_amount(3).unwrap();
        let u10000 = usd.try_amount(10000).unwrap();
        let u2500 = usd.try_amount(2500).unwrap();

        issue(&ledger, "issue-brush", "@store1/inventory", &b10).await;
        issue(&ledger, "issue-usd", "@store1/cash", &u10000).await;

        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", &b3)
            .debit("@store1/cash", &u2500)
            .credit("@customer1", &b3)
            .credit("@customer1", &u2500)
            .build()
            .await
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(
            ledger.balance("@store1/inventory", "brush").await.unwrap(),
            7
        );
        assert_eq!(ledger.balance("@store1/cash", "usd").await.unwrap(), 7500);
        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 3);
        assert_eq!(ledger.balance("@customer1", "usd").await.unwrap(), 2500);
    }

    #[tokio::test]
    async fn credit_sale_via_high_level() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();
        let usd = ledger.asset("usd").unwrap();

        let b5 = brush.try_amount(5).unwrap();
        let b2 = brush.try_amount(2).unwrap();
        let neg_usd = usd.try_amount(-1000).unwrap();
        let pos_usd = usd.try_amount(1000).unwrap();

        issue(&ledger, "issue-001", "@store1/inventory", &b5).await;

        let tx = ledger
            .transaction("sale-001")
            .debit("@store1/inventory", &b2)
            .credit("@customer1", &b2)
            .credit("@customer1", &neg_usd)
            .credit("@store1/receivables", &pos_usd)
            .build()
            .await
            .expect("build tx");
        ledger.commit(tx).await.expect("commit tx");

        assert_eq!(ledger.balance("@customer1", "brush").await.unwrap(), 2);
        assert_eq!(ledger.balance("@customer1", "usd").await.unwrap(), -1000);
        assert_eq!(
            ledger.balance("@store1/inventory", "brush").await.unwrap(),
            3
        );
    }
}
