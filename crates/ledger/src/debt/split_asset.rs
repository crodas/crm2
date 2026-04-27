//! Split-asset debt: debt on a separate `{asset}.d` signed asset.
//!
//! Debt is represented on a *separate* signed asset named `{base}.d` (e.g.,
//! `gs.d` for debts denominated in `gs`). Issue creates paired credits on the
//! debt asset. Settlement *consumes* the debt tokens via explicit UTXO debits,
//! with change outputs for partial payments.

use std::sync::Arc;

use async_trait::async_trait;
use ledger_core::{Amount, Asset, LedgerError, SpendingToken, Storage};

use crate::builder::TransactionBuilder;
use crate::error::Error;
use crate::Ledger;

use super::{resolve_template, DebtStrategy};

/// Debt on a separate `{asset}.d` signed asset.
///
/// Configured with debtor/creditor path templates containing `{from}` and/or `{to}`.
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

    /// Return the debt asset for a base asset (e.g., `"gs"` → `"gs.d"` signed).
    pub fn debt_asset(base: &Asset) -> Asset {
        Asset::new(format!("{}.d", base.name()), base.precision())
    }

    /// Register the debt asset `{base}.d` alongside an existing base asset.
    pub async fn register_debt_asset(
        ledger: &Ledger,
        base_asset: &Asset,
    ) -> Result<(), LedgerError> {
        ledger.register_asset(Self::debt_asset(base_asset)).await
    }

    /// Amount owed by a debtor (returned as positive `i128`).
    pub async fn owed_by(
        &self,
        ledger: &Ledger,
        from: &str,
        to: &str,
        asset: &Asset,
    ) -> Result<i128, Error> {
        let debtor = resolve_template(&self.debtor_template, from, to);
        let debt_asset = Self::debt_asset(asset);
        let balance = ledger.balance(&debtor, debt_asset.name()).await?;
        Ok(balance.unsigned_abs() as i128)
    }

    /// Amount owed to a creditor (returned as positive `i128`).
    pub async fn owed_to(
        &self,
        ledger: &Ledger,
        from: &str,
        to: &str,
        asset: &Asset,
    ) -> Result<i128, Error> {
        let creditor = resolve_template(&self.creditor_template, from, to);
        let debt_asset = Self::debt_asset(asset);
        let balance = ledger.balance(&creditor, debt_asset.name()).await?;
        Ok(balance)
    }
}

#[async_trait]
impl DebtStrategy for SplitAssetDebt {
    fn issue(
        &self,
        builder: TransactionBuilder,
        from: &str,
        to: &str,
        amount: &Amount,
    ) -> Result<TransactionBuilder, Error> {
        if amount.raw() <= 0 {
            return Err(Error::NonPositiveAmount);
        }
        let debtor = resolve_template(&self.debtor_template, from, to);
        let creditor = resolve_template(&self.creditor_template, from, to);
        let debt_asset = Self::debt_asset(amount.asset());
        let neg = debt_asset.try_amount(-amount.raw());
        let pos = debt_asset.try_amount(amount.raw());

        Ok(builder.credit(&debtor, &neg).credit(&creditor, &pos))
    }

    async fn settle(
        &self,
        builder: TransactionBuilder,
        from: &str,
        to: &str,
        amount: &Amount,
    ) -> Result<TransactionBuilder, Error> {
        if amount.raw() <= 0 {
            return Err(Error::NonPositiveAmount);
        }
        let debtor = resolve_template(&self.debtor_template, from, to);
        let creditor = resolve_template(&self.creditor_template, from, to);
        let debt_asset = Self::debt_asset(amount.asset());

        // Select negative tokens from debtor.
        let filter = debt_asset.max();
        let debtor_tokens = self
            .storage
            .unspent_by_account(&debtor, Some(&filter))
            .await?;
        let (selected_debtor, debtor_change) =
            select_negative_tokens(&debtor_tokens, amount.raw())?;

        // Select positive tokens from creditor.
        let creditor_tokens = self
            .storage
            .unspent_by_account(&creditor, Some(&filter))
            .await?;
        let (selected_creditor, creditor_change) =
            select_positive_tokens(&creditor_tokens, amount.raw())?;

        // Add debits for selected tokens via debit_raw.
        let mut b = builder;
        for token in &selected_debtor {
            b = b.debit_raw(
                &token.entry_ref.tx_id,
                token.entry_ref.entry_index,
                &debtor,
                &token.amount,
            );
        }
        for token in &selected_creditor {
            b = b.debit_raw(
                &token.entry_ref.tx_id,
                token.entry_ref.entry_index,
                &creditor,
                &token.amount,
            );
        }

        // Add change credits if partial consumption.
        if let Some(change_raw) = debtor_change {
            let change = debt_asset.try_amount(change_raw);
            b = b.credit(&debtor, &change);
        }
        if let Some(change_raw) = creditor_change {
            let change = debt_asset.try_amount(change_raw);
            b = b.credit(&creditor, &change);
        }

        Ok(b)
    }
}

/// Select negative tokens (debtor side) covering `amount`.
fn select_negative_tokens<'a>(
    tokens: &'a [SpendingToken],
    amount: i128,
) -> Result<(Vec<&'a SpendingToken>, Option<i128>), Error> {
    let mut candidates: Vec<&SpendingToken> =
        tokens.iter().filter(|t| t.amount.raw() < 0).collect();
    candidates.sort_by(|a, b| a.amount.raw().cmp(&b.amount.raw()));

    let mut selected = Vec::new();
    let mut abs_sum: i128 = 0;

    for token in candidates {
        if abs_sum >= amount {
            break;
        }
        selected.push(token);
        abs_sum += token.amount.raw().unsigned_abs() as i128;
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
fn select_positive_tokens<'a>(
    tokens: &'a [SpendingToken],
    amount: i128,
) -> Result<(Vec<&'a SpendingToken>, Option<i128>), Error> {
    let mut candidates: Vec<&SpendingToken> =
        tokens.iter().filter(|t| t.amount.raw() > 0).collect();
    candidates.sort_by(|a, b| b.amount.raw().cmp(&a.amount.raw()));

    let mut selected = Vec::new();
    let mut sum: i128 = 0;

    for token in candidates {
        if sum >= amount {
            break;
        }
        selected.push(token);
        sum += token.amount.raw();
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

    use ledger_core::{Amount, Asset, MemoryStorage};

    use crate::error::Error;
    use crate::Ledger;

    use super::SplitAssetDebt;

    fn gs() -> Asset {
        Asset::new("gs", 0)
    }

    fn gs_amount(raw: i128) -> Amount {
        gs().try_amount(raw)
    }

    async fn setup() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let strategy = SplitAssetDebt::new(storage.clone(), "customer/{from}", "store/{to}");
        let ledger = Ledger::new(storage).with_debt_strategy(strategy);
        ledger.register_asset(gs()).await.unwrap();
        ledger.register_asset(Asset::new("brush", 0)).await.unwrap();
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
            .create_debt("1", "1", &gs_amount(10000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("customer/1", "gs.d").await.unwrap(), -10000);
        assert_eq!(ledger.balance("store/1", "gs.d").await.unwrap(), 10000);
        assert_eq!(ledger.balance("customer/1", "gs").await.unwrap(), 0);
        assert_eq!(ledger.balance("store/1", "gs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_full_zeroes_both() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(10000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("pay-001")
            .settle_debt("1", "1", &gs_amount(10000))
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("customer/1", "gs.d").await.unwrap(), 0);
        assert_eq!(ledger.balance("store/1", "gs.d").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_partial_leaves_remainder() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(10000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("pay-001")
            .settle_debt("1", "1", &gs_amount(6000))
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("customer/1", "gs.d").await.unwrap(), -4000);
        assert_eq!(ledger.balance("store/1", "gs.d").await.unwrap(), 4000);
    }

    #[tokio::test]
    async fn multiple_debts_single_settle() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(5000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("debt-002")
            .create_debt("1", "1", &gs_amount(8000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("pay-001")
            .settle_debt("1", "1", &gs_amount(10000))
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("customer/1", "gs.d").await.unwrap(), -3000);
        assert_eq!(ledger.balance("store/1", "gs.d").await.unwrap(), 3000);
    }

    #[tokio::test]
    async fn overpayment_rejected() {
        let ledger = setup().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(5000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let result = ledger
            .transaction("pay-001")
            .settle_debt("1", "1", &gs_amount(10000))
            .await;
        assert!(matches!(result, Err(Error::InsufficientDebt { .. })));
    }

    #[tokio::test]
    async fn mixed_tx_with_product_debits() {
        let ledger = setup().await;
        let brush = ledger.asset("brush").unwrap();
        let b10 = brush.try_amount(10);
        let b3 = brush.try_amount(3);

        let tx = ledger
            .transaction("issue-inv")
            .issue("store/inventory", &b10)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("sale-001")
            .debit("store/inventory", &b3)
            .credit("customer/1", &b3)
            .create_debt("1", "1", &gs_amount(5000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("store/inventory", "brush").await.unwrap(), 7);
        assert_eq!(ledger.balance("customer/1", "brush").await.unwrap(), 3);
        assert_eq!(ledger.balance("customer/1", "gs.d").await.unwrap(), -5000);
        assert_eq!(ledger.balance("store/1", "gs.d").await.unwrap(), 5000);
    }

    #[tokio::test]
    async fn settle_with_cash_leg() {
        let ledger = setup().await;
        let gs_asset = ledger.asset("gs").unwrap();
        let gs5000 = gs_asset.try_amount(5000);

        let tx = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(10000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("fund-customer")
            .issue("customer/1/cash", &gs5000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        let tx = ledger
            .transaction("pay-001")
            .settle_debt("1", "1", &gs_amount(5000))
            .await
            .unwrap()
            .debit("customer/1/cash", &gs5000)
            .credit("store/cash", &gs5000)
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("customer/1", "gs.d").await.unwrap(), -5000);
        assert_eq!(ledger.balance("store/1", "gs.d").await.unwrap(), 5000);
        assert_eq!(ledger.balance("store/cash", "gs").await.unwrap(), 5000);
        assert_eq!(ledger.balance("customer/1/cash", "gs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn non_positive_amount_rejected() {
        let ledger = setup().await;

        let result = ledger
            .transaction("bad")
            .create_debt("1", "1", &gs_amount(0));
        assert!(matches!(result, Err(Error::NonPositiveAmount)));
    }

    #[tokio::test]
    async fn query_owed_by_and_owed_to() {
        let storage = Arc::new(MemoryStorage::new());
        let debt = SplitAssetDebt::new(storage.clone(), "customer/{from}", "store/{to}");
        let ledger = Ledger::new(storage).with_debt_strategy(SplitAssetDebt::new(
            debt.storage.clone(),
            "customer/{from}",
            "store/{to}",
        ));
        ledger.register_asset(gs()).await.unwrap();
        SplitAssetDebt::register_debt_asset(&ledger, &gs())
            .await
            .unwrap();

        let tx = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(7500))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(debt.owed_by(&ledger, "1", "1", &gs()).await.unwrap(), 7500);
        assert_eq!(debt.owed_to(&ledger, "1", "1", &gs()).await.unwrap(), 7500);

        let tx = ledger
            .transaction("pay-001")
            .settle_debt("1", "1", &gs_amount(3000))
            .await
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(debt.owed_by(&ledger, "1", "1", &gs()).await.unwrap(), 4500);
        assert_eq!(debt.owed_to(&ledger, "1", "1", &gs()).await.unwrap(), 4500);
    }
}
