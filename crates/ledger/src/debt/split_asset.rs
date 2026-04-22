//! Split-asset debt: debt on a separate `{asset}.d` signed asset.
//!
//! # How it works
//!
//! Debt is represented on a *separate* signed asset named `{base}.d` (e.g.,
//! `gs.d` for debts denominated in `gs`). Issue creates paired credits on the
//! debt asset. Settlement *consumes* the debt tokens via explicit UTXO debits,
//! with change outputs for partial payments.
//!
//! # Positives
//!
//! - **Clean separation**: real money (`gs`) and obligations (`gs.d`) live on
//!   different assets. A balance on `gs` is always real funds; a balance on
//!   `gs.d` is always an obligation. No ambiguity.
//! - **Double-spend protection on debt**: settlement consumes UTXO tokens, so
//!   the same debt cannot be settled twice. The ledger's single-spend rule
//!   enforces this automatically.
//! - **Bounded token count**: settled debt tokens are marked spent and excluded
//!   from future queries. Only outstanding debt tokens remain unspent, keeping
//!   balance queries fast.
//! - **Auditability**: each debt token traces back to its originating
//!   transaction, and settlement creates an explicit debit chain. The full
//!   lifecycle is visible in the UTXO graph.
//! - **Query helpers**: `owed_by` and `owed_to` provide direct access to
//!   outstanding amounts without scanning unrelated tokens.
//!
//! # Negatives
//!
//! - **Extra asset registration**: each base asset needs a companion `.d` asset
//!   registered before debt can be issued. Forgetting this is a runtime error.
//! - **UTXO fragmentation**: partial settlements create change tokens. Many
//!   small payments on a large debt produce many small tokens, which may need
//!   consolidation.
//! - **More complex settlement**: settle must perform token selection (scan
//!   unspent tokens, pick enough to cover the amount, compute change). This
//!   adds latency and code compared to the signed-position model.
//! - **Requires storage access for settlement**: the `settle` method needs
//!   `Arc<dyn Storage>` to query tokens, coupling the strategy to the storage
//!   layer at construction time.
//! - **Mixed-transaction ordering**: when combining debt settlement with
//!   product debits in a single transaction, the caller must ensure all debit
//!   entries reference valid unspent tokens — the builder does not coordinate
//!   across the high-level `.debit()` and the raw `.debit_raw()` calls.

use std::sync::Arc;

use async_trait::async_trait;
use ledger_core::{Asset, AssetKind, LedgerError, SpendingToken, Storage};

use crate::builder::TransactionBuilder;
use crate::error::Error;
use crate::Ledger;

use super::{resolve_template, DebtStrategy};

/// Debt on a separate `{asset}.d` signed asset.
///
/// Configured with debtor/creditor path templates containing `{id}`.
///
/// - `issue`: credits debtor with `-amount` and creditor with `+amount`
///   on `{asset}.d` (issuance — conservation satisfied as net zero).
/// - `settle`: debits both sides' `.d` tokens and credits change back,
///   consuming actual UTXO tokens via `debit_raw`.
pub struct SplitAssetDebt {
    pub(crate) storage: Arc<dyn Storage>,
    debtor_template: String,
    creditor_template: String,
}

impl SplitAssetDebt {
    pub fn new(
        storage: Arc<dyn Storage>,
        debtor_template: impl Into<String>,
        creditor_template: impl Into<String>,
    ) -> Self {
        Self {
            storage,
            debtor_template: debtor_template.into(),
            creditor_template: creditor_template.into(),
        }
    }

    /// Return the debt asset name for a base asset (e.g., `"gs"` → `"gs.d"`).
    pub fn debt_asset_name(asset: &Asset) -> String {
        format!("{}.d", asset.name())
    }

    /// Register the debt asset `{base}.d` alongside an existing base asset.
    ///
    /// The debt asset inherits the base asset's precision and is always
    /// `AssetKind::Signed`.
    pub async fn register_debt_asset(
        ledger: &Ledger,
        base_asset: &Asset,
    ) -> Result<(), LedgerError> {
        let debt_name = Self::debt_asset_name(base_asset);
        ledger
            .register_asset(Asset::new(
                debt_name,
                base_asset.precision(),
                AssetKind::Signed,
            ))
            .await
    }

    /// Amount owed by a debtor (returned as positive `i128`).
    ///
    /// Resolves the debtor path from the template and queries the negative
    /// balance on the debt asset, returning its absolute value.
    pub async fn owed_by(
        &self,
        ledger: &Ledger,
        entity_id: i64,
        asset: &Asset,
    ) -> Result<i128, Error> {
        let debtor = resolve_template(&self.debtor_template, &entity_id.to_string())?;
        let debt_name = Self::debt_asset_name(asset);
        let balance = ledger.balance(debtor.as_str(), &debt_name).await?;
        Ok(balance.unsigned_abs() as i128)
    }

    /// Amount owed to a creditor (returned as positive `i128`).
    ///
    /// Resolves the creditor path from the template and queries the positive
    /// balance on the debt asset.
    pub async fn owed_to(
        &self,
        ledger: &Ledger,
        entity_id: i64,
        asset: &Asset,
    ) -> Result<i128, Error> {
        let creditor = resolve_template(&self.creditor_template, &entity_id.to_string())?;
        let debt_name = Self::debt_asset_name(asset);
        let balance = ledger.balance(creditor.as_str(), &debt_name).await?;
        Ok(balance)
    }
}

#[async_trait]
impl DebtStrategy for SplitAssetDebt {
    fn issue(
        &self,
        builder: TransactionBuilder,
        entity_id: &str,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error> {
        if amount <= 0 {
            return Err(Error::NonPositiveAmount);
        }
        let debtor = resolve_template(&self.debtor_template, entity_id)?;
        let creditor = resolve_template(&self.creditor_template, entity_id)?;
        let debt_name = Self::debt_asset_name(asset);
        let neg = asset.from_cents(-amount);
        let pos = asset.from_cents(amount);

        Ok(builder
            .credit(debtor.as_str(), &debt_name, &neg)
            .credit(creditor.as_str(), &debt_name, &pos))
    }

    async fn settle(
        &self,
        builder: TransactionBuilder,
        entity_id: &str,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error> {
        if amount <= 0 {
            return Err(Error::NonPositiveAmount);
        }
        let debtor = resolve_template(&self.debtor_template, entity_id)?;
        let creditor = resolve_template(&self.creditor_template, entity_id)?;
        let debt_name = Self::debt_asset_name(asset);

        // Select negative tokens from debtor.
        let debtor_tokens = self
            .storage
            .unspent_by_account(&debtor, &debt_name)
            .await?;
        let (selected_debtor, debtor_change) =
            select_negative_tokens(&debtor_tokens, amount, asset)?;

        // Select positive tokens from creditor.
        let creditor_tokens = self
            .storage
            .unspent_by_account(&creditor, &debt_name)
            .await?;
        let (selected_creditor, creditor_change) =
            select_positive_tokens(&creditor_tokens, amount, asset)?;

        // Add debits for selected tokens via debit_raw.
        let mut b = builder;
        for token in &selected_debtor {
            b = b.debit_raw(
                &token.entry_ref.tx_id,
                token.entry_ref.entry_index,
                debtor.as_str(),
                &debt_name,
                asset.from_cents(token.qty),
            );
        }
        for token in &selected_creditor {
            b = b.debit_raw(
                &token.entry_ref.tx_id,
                token.entry_ref.entry_index,
                creditor.as_str(),
                &debt_name,
                asset.from_cents(token.qty),
            );
        }

        // Add change credits if partial consumption.
        if let Some(change) = debtor_change {
            b = b.credit(debtor.as_str(), &debt_name, asset.from_cents(change));
        }
        if let Some(change) = creditor_change {
            b = b.credit(creditor.as_str(), &debt_name, asset.from_cents(change));
        }

        Ok(b)
    }
}

/// Select negative tokens (debtor side) covering `amount`.
///
/// Tokens are sorted ascending (most negative first). Accumulates until
/// `abs(sum) >= amount`. Returns the selected tokens and an optional change
/// value (still negative) if the selected tokens exceed the needed amount.
fn select_negative_tokens<'a>(
    tokens: &'a [SpendingToken],
    amount: i128,
    _asset: &Asset,
) -> Result<(Vec<&'a SpendingToken>, Option<i128>), Error> {
    let mut candidates: Vec<&SpendingToken> = tokens.iter().filter(|t| t.qty < 0).collect();
    candidates.sort_by(|a, b| a.qty.cmp(&b.qty));

    let mut selected = Vec::new();
    let mut abs_sum: i128 = 0;

    for token in candidates {
        if abs_sum >= amount {
            break;
        }
        selected.push(token);
        abs_sum += token.qty.unsigned_abs() as i128;
    }

    if abs_sum < amount {
        return Err(Error::InsufficientDebt {
            required: amount,
            available: abs_sum,
        });
    }

    let change = if abs_sum > amount {
        Some(-(abs_sum - amount))
    } else {
        None
    };

    Ok((selected, change))
}

/// Select positive tokens (creditor side) covering `amount`.
///
/// Tokens are sorted descending (largest first). Returns the selected tokens
/// and an optional change value (still positive) if the selected tokens
/// exceed the needed amount.
fn select_positive_tokens<'a>(
    tokens: &'a [SpendingToken],
    amount: i128,
    _asset: &Asset,
) -> Result<(Vec<&'a SpendingToken>, Option<i128>), Error> {
    let mut candidates: Vec<&SpendingToken> = tokens.iter().filter(|t| t.qty > 0).collect();
    candidates.sort_by(|a, b| b.qty.cmp(&a.qty));

    let mut selected = Vec::new();
    let mut sum: i128 = 0;

    for token in candidates {
        if sum >= amount {
            break;
        }
        selected.push(token);
        sum += token.qty;
    }

    if sum < amount {
        return Err(Error::InsufficientDebt {
            required: amount,
            available: sum,
        });
    }

    let change = if sum > amount {
        Some(sum - amount)
    } else {
        None
    };

    Ok((selected, change))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ledger_core::{Asset, AssetKind, MemoryStorage};

    use crate::error::Error;
    use crate::Ledger;

    use super::SplitAssetDebt;

    fn gs() -> Asset {
        Asset::new("gs", 0, AssetKind::Signed)
    }

    async fn setup() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let strategy = SplitAssetDebt::new(
            storage.clone(),
            "@customer/{id}",
            "@store/{id}",
        );
        let ledger = Ledger::new(storage).with_debt_strategy(strategy);
        ledger
            .register_asset(Asset::new("gs", 0, AssetKind::Signed))
            .await
            .unwrap();
        ledger
            .register_asset(Asset::new("brush", 0, AssetKind::Unsigned))
            .await
            .unwrap();
        SplitAssetDebt::register_debt_asset(&ledger, &gs())
            .await
            .unwrap();
        ledger
    }

    #[tokio::test]
    async fn issue_creates_paired_entries_on_debt_asset() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 10000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs.d").await.unwrap(), -10000);
        assert_eq!(ledger.balance("@store/1", "gs.d").await.unwrap(), 10000);
        assert_eq!(ledger.balance("@customer/1", "gs").await.unwrap(), 0);
        assert_eq!(ledger.balance("@store/1", "gs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_full_zeroes_both() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 10000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("pay-001")
            .settle_debt(1, &gs(), 10000)
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs.d").await.unwrap(), 0);
        assert_eq!(ledger.balance("@store/1", "gs.d").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_partial_leaves_remainder() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 10000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("pay-001")
            .settle_debt(1, &gs(), 6000)
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs.d").await.unwrap(), -4000);
        assert_eq!(ledger.balance("@store/1", "gs.d").await.unwrap(), 4000);
    }

    #[tokio::test]
    async fn multiple_debts_single_settle() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 5000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("debt-002")
            .create_debt(1, &gs(), 8000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs.d").await.unwrap(), -13000);
        assert_eq!(ledger.balance("@store/1", "gs.d").await.unwrap(), 13000);

        let tx = ledger
            .transaction("pay-001")
            .settle_debt(1, &gs(), 10000)
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs.d").await.unwrap(), -3000);
        assert_eq!(ledger.balance("@store/1", "gs.d").await.unwrap(), 3000);
    }

    #[tokio::test]
    async fn overpayment_rejected() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 5000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let result = ledger
            .transaction("pay-001")
            .settle_debt(1, &gs(), 10000)
            .await;
        assert!(matches!(result, Err(Error::InsufficientDebt { .. })));
    }

    #[tokio::test]
    async fn mixed_tx_with_product_debits() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("issue-inv")
            .credit("@store/inventory", "brush", "10")
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("sale-001")
            .debit("@store/inventory", "brush", "3")
            .credit("@customer/1", "brush", "3")
            .create_debt(1, &gs(), 5000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@store/inventory", "brush").await.unwrap(), 7);
        assert_eq!(ledger.balance("@customer/1", "brush").await.unwrap(), 3);
        assert_eq!(ledger.balance("@customer/1", "gs.d").await.unwrap(), -5000);
        assert_eq!(ledger.balance("@store/1", "gs.d").await.unwrap(), 5000);
        assert_eq!(ledger.balance("@customer/1", "gs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_with_cash_leg() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 10000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("fund-customer")
            .credit("@customer/1/cash", "gs", "5000")
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("pay-001")
            .settle_debt(1, &gs(), 5000)
            .await
            .unwrap()
            .debit("@customer/1/cash", "gs", "5000")
            .credit("@store/cash", "gs", "5000")
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs.d").await.unwrap(), -5000);
        assert_eq!(ledger.balance("@store/1", "gs.d").await.unwrap(), 5000);
        assert_eq!(ledger.balance("@store/cash", "gs").await.unwrap(), 5000);
        assert_eq!(ledger.balance("@customer/1/cash", "gs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn non_positive_amount_rejected() {
        let ledger = setup().await;

        let result = ledger.transaction("bad").create_debt(1, &gs(), 0);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));
    }

    #[tokio::test]
    async fn query_owed_by_and_owed_to() {
        let storage = Arc::new(MemoryStorage::new());
        let debt = SplitAssetDebt::new(
            storage.clone(),
            "@customer/{id}",
            "@store/{id}",
        );
        let ledger = Ledger::new(storage).with_debt_strategy(SplitAssetDebt::new(
            debt.storage.clone(),
            "@customer/{id}",
            "@store/{id}",
        ));
        ledger
            .register_asset(Asset::new("gs", 0, AssetKind::Signed))
            .await
            .unwrap();
        SplitAssetDebt::register_debt_asset(&ledger, &gs())
            .await
            .unwrap();

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 7500)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(debt.owed_by(&ledger, 1, &gs()).await.unwrap(), 7500);
        assert_eq!(debt.owed_to(&ledger, 1, &gs()).await.unwrap(), 7500);

        let tx = ledger
            .transaction("pay-001")
            .settle_debt(1, &gs(), 3000)
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(debt.owed_by(&ledger, 1, &gs()).await.unwrap(), 4500);
        assert_eq!(debt.owed_to(&ledger, 1, &gs()).await.unwrap(), 4500);
    }
}
