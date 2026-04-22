//! Template-based credit strategy.
//!
//! Configurable with one or two account templates:
//! - `credit_template` — receives the positive amount
//! - `source_template` (optional) — receives the negative amount (paired entry)
//!
//! # Examples
//!
//! Simple cash deposit:
//! ```ignore
//! TemplateCreditStrategy::new("store/cash")
//! ```
//! Produces: `credit("store/cash", +amount)`
//!
//! Deposit via paypal:
//! ```ignore
//! TemplateCreditStrategy::paired("store/cash", "store/paypal_receivable/{id}")
//! ```
//! Produces: `credit("store/cash", +amount), credit("store/paypal_receivable/42", -amount)`

use ledger_core::Amount;

use super::CreditStrategy;
use crate::builder::TransactionBuilder;
use crate::debt::resolve_template;
use crate::error::Error;

/// A template-based [`CreditStrategy`] that credits one account and
/// optionally debits another via a negative credit.
pub struct TemplateCreditStrategy {
    credit_template: String,
    source_template: Option<String>,
}

impl TemplateCreditStrategy {
    /// Create a strategy that credits a single account.
    ///
    /// The `{id}` placeholder in the template is replaced with the entity
    /// identifier at call time.
    pub fn new(credit_template: impl Into<String>) -> Self {
        Self {
            credit_template: credit_template.into(),
            source_template: None,
        }
    }

    /// Create a strategy that credits one account and adds a paired
    /// negative credit to a source account.
    ///
    /// This is useful for deposits where you want to track the source
    /// (e.g., paypal receivable, bank transfer pending).
    pub fn paired(credit_template: impl Into<String>, source_template: impl Into<String>) -> Self {
        Self {
            credit_template: credit_template.into(),
            source_template: Some(source_template.into()),
        }
    }
}

impl CreditStrategy for TemplateCreditStrategy {
    fn apply(
        &self,
        builder: TransactionBuilder,
        entity_id: &str,
        amount: &Amount,
    ) -> Result<TransactionBuilder, Error> {
        if amount.raw() <= 0 {
            return Err(Error::NonPositiveAmount);
        }

        let credit_account = resolve_template(&self.credit_template, entity_id);
        let mut b = builder.credit(&credit_account, amount);

        if let Some(ref source_tmpl) = self.source_template {
            let source_account = resolve_template(source_tmpl, entity_id);
            let neg = amount
                .asset()
                .try_amount(-amount.raw())
                .map_err(Error::Ledger)?;
            b = b.credit(&source_account, &neg);
        }

        Ok(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use ledger_core::{Asset, AssetKind, MemoryStorage};

    use crate::Ledger;

    fn gs() -> Asset {
        Asset::new("gs", 0, AssetKind::Signed)
    }

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage);
        ledger.register_asset(gs()).await.unwrap();
        ledger
    }

    #[tokio::test]
    async fn simple_credit() {
        let ledger = setup_ledger().await;
        let strategy = TemplateCreditStrategy::new("store/cash");
        let amount = gs().try_amount(5000).unwrap();

        let builder = ledger.transaction("deposit-001");
        let builder = strategy.apply(builder, "1", &amount).unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("store/cash", "gs").await.unwrap(), 5000);
    }

    #[tokio::test]
    async fn simple_credit_with_id() {
        let ledger = setup_ledger().await;
        let strategy = TemplateCreditStrategy::new("store/{id}/cash");
        let amount = gs().try_amount(3000).unwrap();

        let builder = ledger.transaction("deposit-001");
        let builder = strategy.apply(builder, "warehouse1", &amount).unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(
            ledger.balance("store/warehouse1/cash", "gs").await.unwrap(),
            3000
        );
    }

    #[tokio::test]
    async fn paired_credit() {
        let ledger = setup_ledger().await;
        let strategy = TemplateCreditStrategy::paired("store/cash", "store/paypal_receivable/{id}");
        let amount = gs().try_amount(10000).unwrap();

        let builder = ledger.transaction("paypal-001");
        let builder = strategy.apply(builder, "tx_abc", &amount).unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("store/cash", "gs").await.unwrap(), 10000);
        assert_eq!(
            ledger
                .balance("store/paypal_receivable/tx_abc", "gs")
                .await
                .unwrap(),
            -10000
        );
    }

    #[tokio::test]
    async fn rejects_non_positive_amount() {
        let ledger = setup_ledger().await;
        let strategy = TemplateCreditStrategy::new("store/cash");

        let zero = gs().try_amount(0).unwrap();
        let result = strategy.apply(ledger.transaction("bad"), "1", &zero);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));

        let neg = gs().try_amount(-100).unwrap();
        let result = strategy.apply(ledger.transaction("bad2"), "1", &neg);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));
    }
}
