//! Step 1 — Mark input tokens as spent.
//!
//! Flags every referenced UTXO as consumed by the new transaction.
//! On compensation, restores them to unspent so they remain available
//! for future transactions.

use async_trait::async_trait;
use legend::step::{CompensationOutcome, Step, StepOutcome};
use serde::{Deserialize, Serialize};

use super::context::CommitCtx;
use crate::error::LedgerError;
use crate::token::CreditEntryRef;

/// Marker type for the mark-spent saga step.
pub struct MarkSpentStep;

/// Input data: which tokens to mark and which transaction consumes them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSpentInput {
    pub spent_refs: Vec<CreditEntryRef>,
    pub by_tx: String,
}

#[async_trait]
impl Step<CommitCtx, LedgerError> for MarkSpentStep {
    type Input = MarkSpentInput;

    /// Flag each referenced token as spent in storage.
    async fn execute(ctx: &mut CommitCtx, input: &Self::Input) -> Result<StepOutcome, LedgerError> {
        ctx.storage
            .mark_spent(&input.spent_refs, &input.by_tx)
            .await?;
        Ok(StepOutcome::Continue)
    }

    /// Undo: restore every flagged token back to unspent.
    async fn compensate(
        ctx: &mut CommitCtx,
        input: &Self::Input,
    ) -> Result<CompensationOutcome, LedgerError> {
        ctx.storage.unmark_spent(&input.spent_refs).await?;
        Ok(CompensationOutcome::Completed)
    }
}
