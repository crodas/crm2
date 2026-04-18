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
use ledger_core::{AccountPath, Asset};

use crate::builder::TransactionBuilder;
use crate::error::Error;

use super::DebtStrategy;

/// Debt on the same asset using the signed-position model.
///
/// - `issue`: credits debtor with `-amount` and creditor with `+amount`
///   on the *same* asset (issuance — no debits needed).
/// - `settle`: credits debtor with `+amount` and creditor with `-amount`
///   (issuance that offsets the original position).
pub struct SignedPositionDebt;

#[async_trait]
impl DebtStrategy for SignedPositionDebt {
    fn issue(
        &self,
        builder: TransactionBuilder,
        debtor: &AccountPath,
        creditor: &AccountPath,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error> {
        if amount <= 0 {
            return Err(Error::NonPositiveAmount);
        }
        let neg = asset.from_cents(-amount);
        let pos = asset.from_cents(amount);
        let asset_name = asset.name();

        Ok(builder.credit(debtor.as_str(), asset_name, &neg).credit(
            creditor.as_str(),
            asset_name,
            &pos,
        ))
    }

    async fn settle(
        &self,
        builder: TransactionBuilder,
        debtor: &AccountPath,
        creditor: &AccountPath,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error> {
        if amount <= 0 {
            return Err(Error::NonPositiveAmount);
        }
        // Issuance that offsets the original position.
        let pos = asset.from_cents(amount);
        let neg = asset.from_cents(-amount);
        let asset_name = asset.name();

        Ok(builder.credit(debtor.as_str(), asset_name, &pos).credit(
            creditor.as_str(),
            asset_name,
            &neg,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ledger_core::{AccountPath, Asset, AssetKind, MemoryStorage};

    use crate::error::Error;
    use crate::Ledger;

    use super::SignedPositionDebt;

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage).with_debt_strategy(SignedPositionDebt);
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

    fn debtor() -> AccountPath {
        AccountPath::new("@customer/1").unwrap()
    }

    fn creditor() -> AccountPath {
        AccountPath::new("@store").unwrap()
    }

    #[tokio::test]
    async fn no_strategy_returns_error() {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage); // no strategy
        ledger
            .register_asset(Asset::new("gs", 0, AssetKind::Signed))
            .await
            .unwrap();

        let builder = ledger.transaction("debt-001");
        let result = ledger.issue_debt(builder, &debtor(), &creditor(), &gs(), 10000);
        assert!(matches!(result, Err(Error::NoDebtStrategy)));
    }

    #[tokio::test]
    async fn issue_creates_paired_entries() {
        let ledger = setup_ledger().await;

        let builder = ledger.transaction("debt-001");
        let builder = ledger
            .issue_debt(builder, &debtor(), &creditor(), &gs(), 10000)
            .unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance(&debtor(), "gs").await.unwrap(), -10000);
        assert_eq!(ledger.balance(&creditor(), "gs").await.unwrap(), 10000);
    }

    #[tokio::test]
    async fn settle_full_zeroes_both() {
        let ledger = setup_ledger().await;

        let builder = ledger.transaction("debt-001");
        let builder = ledger
            .issue_debt(builder, &debtor(), &creditor(), &gs(), 10000)
            .unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        let builder = ledger.transaction("pay-001");
        let builder = ledger
            .settle_debt(builder, &debtor(), &creditor(), &gs(), 10000)
            .await
            .unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance(&debtor(), "gs").await.unwrap(), 0);
        assert_eq!(ledger.balance(&creditor(), "gs").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_partial_leaves_remainder() {
        let ledger = setup_ledger().await;

        let builder = ledger.transaction("debt-001");
        let builder = ledger
            .issue_debt(builder, &debtor(), &creditor(), &gs(), 10000)
            .unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        let builder = ledger.transaction("pay-001");
        let builder = ledger
            .settle_debt(builder, &debtor(), &creditor(), &gs(), 6000)
            .await
            .unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance(&debtor(), "gs").await.unwrap(), -4000);
        assert_eq!(ledger.balance(&creditor(), "gs").await.unwrap(), 4000);
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

        let builder = ledger
            .transaction("sale-001")
            .debit("@store/inventory", "brush", "3")
            .credit("@customer/1", "brush", "3");

        let builder = ledger
            .issue_debt(builder, &debtor(), &creditor(), &gs(), 5000)
            .unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        let inv = AccountPath::new("@store/inventory").unwrap();
        assert_eq!(ledger.balance(&inv, "brush").await.unwrap(), 7);
        assert_eq!(ledger.balance(&debtor(), "brush").await.unwrap(), 3);
        assert_eq!(ledger.balance(&debtor(), "gs").await.unwrap(), -5000);
        assert_eq!(ledger.balance(&creditor(), "gs").await.unwrap(), 5000);
    }

    #[tokio::test]
    async fn settle_with_cash_credit() {
        let ledger = setup_ledger().await;

        let builder = ledger.transaction("debt-001");
        let builder = ledger
            .issue_debt(builder, &debtor(), &creditor(), &gs(), 10000)
            .unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        let builder = ledger.transaction("pay-001");
        let builder = ledger
            .settle_debt(builder, &debtor(), &creditor(), &gs(), 5000)
            .await
            .unwrap();
        let builder = builder.credit("@store/cash", "gs", "5000");
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance(&debtor(), "gs").await.unwrap(), -5000);
        assert_eq!(ledger.balance(&creditor(), "gs").await.unwrap(), 5000);
        let cash = AccountPath::new("@store/cash").unwrap();
        assert_eq!(ledger.balance(&cash, "gs").await.unwrap(), 5000);
    }

    #[tokio::test]
    async fn non_positive_amount_rejected() {
        let ledger = setup_ledger().await;

        let builder = ledger.transaction("bad");
        let result = ledger.issue_debt(builder, &debtor(), &creditor(), &gs(), 0);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));

        let builder = ledger.transaction("bad2");
        let result = ledger.issue_debt(builder, &debtor(), &creditor(), &gs(), -100);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));
    }
}
