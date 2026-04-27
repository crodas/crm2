//! Signed-position debt: debt on the same asset using negative amounts.
//!
//! Debt is represented as paired credits on the *same* monetary asset:
//! - Debtor receives a **negative** credit (they owe money).
//! - Creditor receives a **positive** credit (they are owed money).
//!
//! Settlement is another pair of credits with reversed signs.

use async_trait::async_trait;
use ledger_core::Amount;

use crate::builder::TransactionBuilder;
use crate::error::Error;

use super::{resolve_template, DebtStrategy};

/// Debt on the same asset using the signed-position model.
///
/// Configured with debtor/creditor path templates containing `{from}` and/or `{to}`.
pub struct SignedPositionDebt {
    debtor_template: String,
    creditor_template: String,
}

impl SignedPositionDebt {
    pub fn new(debtor_template: impl Into<String>, creditor_template: impl Into<String>) -> Self {
        Self {
            debtor_template: debtor_template.into(),
            creditor_template: creditor_template.into(),
        }
    }
}

#[async_trait]
impl DebtStrategy for SignedPositionDebt {
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
        let neg = amount.negate();

        Ok(builder.credit(debtor, &neg).credit(creditor, amount))
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
        let neg = amount.negate();

        Ok(builder.credit(debtor, amount).credit(creditor, &neg))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ledger_core::{Amount, Asset, MemoryStorage};

    use crate::error::Error;
    use crate::Ledger;

    use super::SignedPositionDebt;

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage)
            .with_debt_strategy(SignedPositionDebt::new("customer/{from}", "store/{to}"));
        ledger.register_asset(Asset::new("gs", 0)).await.unwrap();
        ledger.register_asset(Asset::new("brush", 0)).await.unwrap();
        ledger
    }

    fn gs_amount(raw: i128) -> Amount {
        Asset::new("gs", 0).try_amount(raw).unwrap()
    }

    #[tokio::test]
    async fn no_strategy_returns_error() {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage); // no strategy
        ledger.register_asset(Asset::new("gs", 0)).await.unwrap();

        let result = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(10000));
        assert!(matches!(result, Err(Error::NoDebtStrategy)));
    }

    #[tokio::test]
    async fn issue_creates_paired_entries() {
        let ledger = setup_ledger().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt("1", "1", &gs_amount(10000))
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("customer/1").await.unwrap()["gs"].raw(), -10000);
        assert_eq!(ledger.balance("store/1").await.unwrap()["gs"].raw(), 10000);
    }

    #[tokio::test]
    async fn settle_full_zeroes_both() {
        let ledger = setup_ledger().await;

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

        assert_eq!(ledger.balance("customer/1").await.unwrap().get("gs").map_or(0, |a| a.raw()), 0);
        assert_eq!(ledger.balance("store/1").await.unwrap().get("gs").map_or(0, |a| a.raw()), 0);
    }

    #[tokio::test]
    async fn settle_partial_leaves_remainder() {
        let ledger = setup_ledger().await;

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

        assert_eq!(ledger.balance("customer/1").await.unwrap()["gs"].raw(), -4000);
        assert_eq!(ledger.balance("store/1").await.unwrap()["gs"].raw(), 4000);
    }

    #[tokio::test]
    async fn mixed_tx_with_product_debits() {
        let ledger = setup_ledger().await;
        let brush = ledger.asset("brush").unwrap();
        let b10 = brush.try_amount(10).unwrap();
        let b3 = brush.try_amount(3).unwrap();

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

        assert_eq!(ledger.balance("store/inventory").await.unwrap()["brush"].raw(), 7);
        assert_eq!(ledger.balance("customer/1").await.unwrap()["brush"].raw(), 3);
        assert_eq!(ledger.balance("customer/1").await.unwrap()["gs"].raw(), -5000);
        assert_eq!(ledger.balance("store/1").await.unwrap()["gs"].raw(), 5000);
    }

    #[tokio::test]
    async fn settle_with_cash_credit() {
        let ledger = setup_ledger().await;
        let gs = ledger.asset("gs").unwrap();
        let gs5000 = gs.try_amount(5000).unwrap();

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
            .settle_debt("1", "1", &gs_amount(5000))
            .await
            .unwrap()
            .issue("store/cash", &gs5000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("customer/1").await.unwrap()["gs"].raw(), -5000);
        assert_eq!(ledger.balance("store/1").await.unwrap()["gs"].raw(), 5000);
        assert_eq!(ledger.balance("store/cash").await.unwrap()["gs"].raw(), 5000);
    }

    #[tokio::test]
    async fn non_positive_amount_rejected() {
        let ledger = setup_ledger().await;

        let result = ledger
            .transaction("bad")
            .create_debt("1", "1", &gs_amount(0));
        assert!(matches!(result, Err(Error::NonPositiveAmount)));

        let neg = Asset::new("gs", 0).try_amount(-100).unwrap();
        let result = ledger.transaction("bad2").create_debt("1", "1", &neg);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));
    }
}
