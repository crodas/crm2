//! Template-based issuance strategy.
//!
//! Credits the destination account and debits a configured source account
//! (e.g., `@world`) so that conservation always holds.
//!
//! # Examples
//!
//! ```ignore
//! let strategy = TemplateIssuanceStrategy::new("@world");
//! // builder.issue("store/cash", &amount) produces:
//! //   credit("store/cash", +amount), credit("@world", -amount)
//! ```

use ledger_core::Amount;

use super::IssuanceStrategy;
use crate::builder::TransactionBuilder;
use crate::error::Error;

/// A template-based [`IssuanceStrategy`] that credits the destination
/// and adds a negative credit to the configured source account.
pub struct TemplateIssuanceStrategy {
    source: String,
}

impl TemplateIssuanceStrategy {
    /// Create a strategy that issues tokens from the given source account.
    ///
    /// The source receives a negative credit (tracking total issuance).
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }
}

impl IssuanceStrategy for TemplateIssuanceStrategy {
    fn apply(
        &self,
        builder: TransactionBuilder,
        to: &str,
        amount: &Amount,
    ) -> Result<TransactionBuilder, Error> {
        if amount.raw() <= 0 {
            return Err(Error::NonPositiveAmount);
        }

        let neg = amount.negate();
        Ok(builder.credit(to, amount).credit(&self.source, &neg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use ledger_core::{Asset, MemoryStorage};

    use crate::Ledger;

    fn gs() -> Asset {
        Asset::new("gs", 0)
    }

    async fn setup_ledger() -> Ledger {
        let storage = Arc::new(MemoryStorage::new());
        let ledger = Ledger::new(storage);
        ledger.register_asset(gs()).await.unwrap();
        ledger
    }

    #[tokio::test]
    async fn issue_credits_destination_and_debits_world() {
        let ledger = setup_ledger().await;
        let strategy = TemplateIssuanceStrategy::new("@world");
        let amount = gs().try_amount(5000);

        let builder = ledger.transaction("deposit-001");
        let builder = strategy.apply(builder, "store/cash", &amount).unwrap();
        let tx = builder.build().await.unwrap();
        ledger.commit(tx).await.unwrap();

        assert_eq!(ledger.balance("store/cash", "gs").await.unwrap(), 5000);
        assert_eq!(ledger.balance("@world", "gs").await.unwrap(), -5000);
    }

    #[tokio::test]
    async fn rejects_non_positive_amount() {
        let ledger = setup_ledger().await;
        let strategy = TemplateIssuanceStrategy::new("@world");

        let zero = gs().try_amount(0);
        let result = strategy.apply(ledger.transaction("bad"), "store/cash", &zero);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));

        let neg = gs().try_amount(-100);
        let result = strategy.apply(ledger.transaction("bad2"), "store/cash", &neg);
        assert!(matches!(result, Err(Error::NonPositiveAmount)));
    }
}
