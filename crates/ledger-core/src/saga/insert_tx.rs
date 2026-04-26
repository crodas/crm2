//! Step 3 — Insert the transaction record.
//!
//! Persists the serialized transaction and its idempotency key.
//! On compensation, removes both so the transaction is fully rolled back
//! and the idempotency key is freed for a future retry.

use async_trait::async_trait;
use legend::step::{CompensationOutcome, Step, StepOutcome};
use serde::{Deserialize, Serialize};

use super::context::CommitCtx;
use crate::error::LedgerError;
use crate::transaction::Transaction;

/// Marker type for the insert-transaction saga step.
pub struct InsertTxStep;

/// Input data: the full transaction to persist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertTxInput {
    pub tx: Transaction,
}

#[async_trait]
impl Step<CommitCtx, LedgerError> for InsertTxStep {
    type Input = InsertTxInput;

    /// Persist the transaction record and its idempotency key.
    async fn execute(ctx: &mut CommitCtx, input: &Self::Input) -> Result<StepOutcome, LedgerError> {
        ctx.storage.insert_tx(&input.tx).await?;
        Ok(StepOutcome::Continue)
    }

    /// Undo: remove the transaction record and free the idempotency key.
    async fn compensate(
        ctx: &mut CommitCtx,
        input: &Self::Input,
    ) -> Result<CompensationOutcome, LedgerError> {
        ctx.storage.remove_tx(&input.tx.tx_id).await?;
        Ok(CompensationOutcome::Completed)
    }
}
