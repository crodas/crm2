//! Step 2 — Create new output tokens.
//!
//! Inserts the credit tokens produced by the transaction (one per credit
//! entry). On compensation, removes them so no phantom tokens remain
//! in storage.

use async_trait::async_trait;
use legend::step::{CompensationOutcome, Step, StepOutcome};
use serde::{Deserialize, Serialize};

use super::context::CommitCtx;
use crate::error::LedgerError;
use crate::token::{CreditEntryRef, CreditToken};

/// Marker type for the create-tokens saga step.
pub struct CreateTokensStep;

/// Input data: the tokens to insert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTokensInput {
    pub tokens: Vec<CreditToken>,
}

#[async_trait]
impl Step<CommitCtx, LedgerError> for CreateTokensStep {
    type Input = CreateTokensInput;

    /// Insert all new output tokens into storage.
    async fn execute(ctx: &mut CommitCtx, input: &Self::Input) -> Result<StepOutcome, LedgerError> {
        ctx.storage.insert_tokens(&input.tokens).await?;
        Ok(StepOutcome::Continue)
    }

    /// Undo: delete the tokens that were just created.
    async fn compensate(
        ctx: &mut CommitCtx,
        input: &Self::Input,
    ) -> Result<CompensationOutcome, LedgerError> {
        let refs: Vec<CreditEntryRef> = input.tokens.iter().map(|t| t.entry_ref.clone()).collect();
        ctx.storage.remove_tokens(&refs).await?;
        Ok(CompensationOutcome::Completed)
    }
}
