//! Signed-position debt: debt on the same asset using negative amounts.
//!
//! # How it works
//!
//! Debt is represented as paired credits on the *same* monetary asset:
//! - Debtor receives a **negative** credit (they owe money).
//! - Creditor receives a **positive** credit (they are owed money).
//!
//! Settlement is another pair of credits with reversed signs. No UTXO
//! consumption is needed — the balance simply shifts via new issuance entries.
//!
//! # Positives
//!
//! - **Simple**: no extra assets to register, no token selection for settlement.
//!   Issue and settle are both pure issuance (credit-only) transactions.
//! - **Single balance query**: `balance_prefix(@customer/{id}, "gs")` gives
//!   the net position directly — negative means they owe, positive means
//!   overpaid.
//! - **Mixed transactions**: debt entries can be added to any transaction
//!   alongside product debits/credits without extra complexity.
//! - **No UTXO fragmentation**: settlement doesn't consume tokens, so there
//!   are no change outputs or growing token counts.
//!
//! # Negatives
//!
//! - **Unbounded token accumulation**: every issue and settle creates new
//!   tokens that are never spent. Over time, balance queries must scan more
//!   tokens. For high-volume systems this can degrade performance.
//! - **No double-spend protection on debt**: since tokens are never consumed,
//!   there is no mechanism to prevent settling more than owed — the caller
//!   must enforce this externally.
//! - **Mixes debt with real money**: the same asset (`gs`) represents both
//!   actual cash and obligations. A positive balance on a creditor account
//!   could be either real funds or an uncollected receivable — the ledger
//!   can't distinguish them.
//! - **Audit complexity**: reconstructing who owes what requires summing all
//!   tokens, since individual issue/settle events are independent entries
//!   rather than linked UTXO chains.

use async_trait::async_trait;
use ledger_core::Asset;

use crate::builder::TransactionBuilder;
use crate::error::Error;

use super::{resolve_template, DebtStrategy};

/// Debt on the same asset using the signed-position model.
///
/// Configured with debtor/creditor path templates containing `{id}`.
///
/// - `issue`: credits debtor with `-amount` and creditor with `+amount`
///   on the *same* asset (issuance — no debits needed).
/// - `settle`: credits debtor with `+amount` and creditor with `-amount`
///   (issuance that offsets the original position).
pub struct SignedPositionDebt {
    debtor_template: String,
    creditor_template: String,
}

impl SignedPositionDebt {
    pub fn new(
        debtor_template: impl Into<String>,
        creditor_template: impl Into<String>,
    ) -> Self {
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
        entity_id: &str,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error> {
        if amount <= 0 {
            return Err(Error::NonPositiveAmount);
        }
        let debtor = resolve_template(&self.debtor_template, entity_id)?;
        let creditor = resolve_template(&self.creditor_template, entity_id)?;
        let neg = asset.from_cents(-amount);
        let pos = asset.from_cents(amount);
        let asset_name = asset.name();

        Ok(builder
            .credit(debtor.as_str(), asset_name, &neg)
            .credit(creditor.as_str(), asset_name, &pos))
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
        let pos = asset.from_cents(amount);
        let neg = asset.from_cents(-amount);
        let asset_name = asset.name();

        Ok(builder
            .credit(debtor.as_str(), asset_name, &pos)
            .credit(creditor.as_str(), asset_name, &neg))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ledger_core::{Asset, AssetKind, MemoryStorage};

    use crate::error::Error;
    use crate::Ledger;

    use super::SignedPositionDebt;

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage).with_debt_strategy(SignedPositionDebt::new(
            "@customer/{id}",
            "@store/{id}",
        ));
        ledger
            .register_asset(Asset::new("gs", 0, AssetKind::Signed))
            .await
            .unwrap();
        ledger
            .register_asset(Asset::new("brush", 0, AssetKind::Unsigned))
            .await
            .unwrap();
        ledger
    }

    fn gs() -> Asset {
        Asset::new("gs", 0, AssetKind::Signed)
    }

    #[tokio::test]
    async fn no_strategy_returns_error() {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage); // no strategy
        ledger
            .register_asset(Asset::new("gs", 0, AssetKind::Signed))
            .await
            .unwrap();

        let result = ledger.transaction("debt-001").create_debt(1, &gs(), 10000);
        assert!(matches!(result, Err(Error::NoDebtStrategy)));
    }

    #[tokio::test]
    async fn issue_creates_paired_entries() {
        let ledger = setup_ledger().await;

        let tx = ledger
            .transaction("debt-001")
            .create_debt(1, &gs(), 10000)
            .unwrap()
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs").await.unwrap(), -10000);
        assert_eq!(ledger.balance("@store/1", "gs").await.unwrap(), 10000);
    }

    #[tokio::test]
    async fn settle_full_zeroes_both() {
        let ledger = setup_ledger().await;

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

        assert_eq!(ledger.balance("@customer/1", "gs").await.unwrap(), 0);
        assert_eq!(ledger.balance("@store/1", "gs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_partial_leaves_remainder() {
        let ledger = setup_ledger().await;

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

        assert_eq!(ledger.balance("@customer/1", "gs").await.unwrap(), -4000);
        assert_eq!(ledger.balance("@store/1", "gs").await.unwrap(), 4000);
    }

    #[tokio::test]
    async fn mixed_tx_with_product_debits() {
        let ledger = setup_ledger().await;

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
        assert_eq!(ledger.balance("@customer/1", "gs").await.unwrap(), -5000);
        assert_eq!(ledger.balance("@store/1", "gs").await.unwrap(), 5000);
    }

    #[tokio::test]
    async fn settle_with_cash_credit() {
        let ledger = setup_ledger().await;

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
            .settle_debt(1, &gs(), 5000)
            .await
            .unwrap()
            .credit("@store/cash", "gs", "5000")
            .build()
            .await
            .unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("@customer/1", "gs").await.unwrap(), -5000);
        assert_eq!(ledger.balance("@store/1", "gs").await.unwrap(), 5000);
        assert_eq!(ledger.balance("@store/cash", "gs").await.unwrap(), 5000);
    }

    #[tokio::test]
    async fn non_positive_amount_rejected() {
        let ledger = setup_ledger().await;

        let result = ledger.transaction("bad").create_debt(1, &gs(), 0);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));

        let result = ledger.transaction("bad2").create_debt(1, &gs(), -100);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));
    }
}
