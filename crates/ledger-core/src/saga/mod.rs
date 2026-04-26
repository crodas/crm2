//! Saga-based commit pipeline.
//!
//! Models the ledger commit as a three-step saga using [`legend`]:
//!
//! 1. **Mark spent** — flag input tokens as spent (compensate: unmark).
//! 2. **Create tokens** — insert new output tokens (compensate: remove).
//! 3. **Insert transaction** — persist the transaction record (compensate: remove).
//!
//! If any step fails, previously completed steps are compensated in reverse
//! order, leaving storage in its original state.
//!
//! ## Architecture
//!
//! Each step lives in its own submodule with a marker type (e.g.
//! [`MarkSpentStep`]) and an input struct. The [`legend::legend!`] macro
//! composes them into a [`CommitSaga`] that the [`run_commit`] function
//! builds and executes against a [`CommitCtx`].

mod context;
mod create_tokens;
mod insert_tx;
mod mark_spent;

use std::sync::Arc;

use context::CommitCtx;
use create_tokens::{CreateTokensInput, CreateTokensStep};
use insert_tx::{InsertTxInput, InsertTxStep};
use mark_spent::{MarkSpentInput, MarkSpentStep};

use crate::error::LedgerError;
use crate::storage::Storage;
use crate::token::{CreditEntryRef, CreditToken};
use crate::transaction::Transaction;

// ── Saga definition ────────────────────────────────────────────────
//
// Composes the three steps into a single saga. The `legend!` macro
// generates `CommitSaga`, `CommitSagaInputs`, and the HList type alias
// that wires the steps together at compile time.

legend::legend! {
    CommitSaga<CommitCtx, LedgerError> {
        mark_spent: MarkSpentStep,
        create_tokens: CreateTokensStep,
        insert_tx: InsertTxStep,
    }
}

/// Run the commit saga: mark spent → create tokens → insert transaction.
///
/// On failure, all completed steps are compensated in reverse order so
/// storage is left unchanged.
pub(crate) async fn run_commit(
    storage: Arc<dyn Storage>,
    spent_refs: Vec<CreditEntryRef>,
    new_tokens: Vec<CreditToken>,
    tx: Transaction,
) -> Result<String, LedgerError> {
    let tx_id = tx.tx_id.clone();

    let saga = CommitSaga::new(CommitSagaInputs {
        mark_spent: MarkSpentInput {
            spent_refs,
            by_tx: tx_id.clone(),
        },
        create_tokens: CreateTokensInput { tokens: new_tokens },
        insert_tx: InsertTxInput { tx },
    });

    let ctx = CommitCtx { storage };
    let execution = saga.build(ctx);

    match execution.start().await {
        legend::execution::ExecutionResult::Completed(_) => Ok(tx_id),
        legend::execution::ExecutionResult::Failed(_, err) => Err(err),
        legend::execution::ExecutionResult::CompensationFailed {
            original_error,
            compensation_error,
            failed_at,
            ..
        } => {
            tracing::error!(
                step = failed_at,
                %original_error,
                %compensation_error,
                "saga compensation failed — ledger may be inconsistent"
            );
            Err(LedgerError::CompensationFailed {
                original: Box::new(original_error),
                compensation: Box::new(compensation_error),
                step: failed_at,
            })
        }
        legend::execution::ExecutionResult::Paused(_) => {
            unreachable!("commit saga never pauses")
        }
    }
}
