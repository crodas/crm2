//! Step 2 — Create new output credit tokens.
//!
//! Inserts the credit tokens produced by the transaction (one per credit
//! entry). On compensation, removes them so no phantom tokens remain
//! in storage.

use async_trait::async_trait;
use legend::step::{CompensationOutcome, Step, StepOutcome};
use serde::{Deserialize, Serialize};

use super::context::CommitCtx;
use crate::error::LedgerError;
use crate::credit_token::{CreditEntryRef, CreditToken};

/// Marker type for the create-credit-tokens saga step.
pub struct CreateCreditTokensStep;

/// Input data: the credit tokens to insert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCreditTokensInput {
    pub credit_tokens: Vec<CreditToken>,
}

#[async_trait]
impl Step<CommitCtx, LedgerError> for CreateCreditTokensStep {
    type Input = CreateCreditTokensInput;

    /// Insert all new output credit tokens into storage.
    async fn execute(ctx: &mut CommitCtx, input: &Self::Input) -> Result<StepOutcome, LedgerError> {
        ctx.storage.insert_credit_tokens(&input.credit_tokens).await?;
        Ok(StepOutcome::Continue)
    }

    /// Undo: delete the credit tokens that were just created.
    async fn compensate(
        ctx: &mut CommitCtx,
        input: &Self::Input,
    ) -> Result<CompensationOutcome, LedgerError> {
        let refs: Vec<CreditEntryRef> = input.credit_tokens.iter().map(|t| t.entry_ref.clone()).collect();
        ctx.storage.remove_credit_tokens(&refs).await?;
        Ok(CompensationOutcome::Completed)
    }
}
