//! Error types for ledger validation.

use serde::{Deserialize, Serialize};

use crate::token::CreditEntryRef;

/// All errors that can occur during ledger operations.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum LedgerError {
    /// A debit references a spending token that does not exist.
    #[error("debit {0} not found in ledger")]
    DebitNotFound(CreditEntryRef),

    /// A debit references a spending token that has already been spent.
    #[error("debit {0} has already been spent")]
    AlreadySpent(CreditEntryRef),

    /// The asset named in the transaction is not registered.
    #[error("unknown asset: {0}")]
    UnknownAsset(String),

    /// Conservation violated: debit and credit sums do not match for an asset.
    #[error("conservation violated for asset '{asset}': debits sum to {debit_sum}, credits sum to {credit_sum}")]
    ConservationViolated {
        asset: String,
        debit_sum: i128,
        credit_sum: i128,
    },

    /// A negative credit exists without a matching positive credit
    /// in the same transaction (invariant 5).
    #[error("dangling debt: negative credit for '{asset}' without matching positive credit")]
    DanglingDebt { asset: String },

    /// The computed transaction ID does not match the stored one.
    #[error("transaction ID mismatch: computed {computed}, stored {stored}")]
    TxIdMismatch { computed: String, stored: String },

    /// A transaction with this idempotency key already exists.
    #[error("duplicate idempotency key: {0}")]
    DuplicateIdempotencyKey(String),

    /// The debit's owner does not match the token it references.
    #[error("debit owner mismatch at {entry_ref}: expected {expected}, got {got}")]
    DebitOwnerMismatch {
        entry_ref: CreditEntryRef,
        expected: String,
        got: String,
    },

    /// The debit's asset does not match the token it references.
    #[error("debit asset mismatch at {entry_ref}: expected {expected}, got {got}")]
    DebitAssetMismatch {
        entry_ref: CreditEntryRef,
        expected: String,
        got: String,
    },

    /// The debit's quantity does not match the token it references.
    #[error("debit qty mismatch at {entry_ref}: expected {expected}, got {got}")]
    DebitQtyMismatch {
        entry_ref: CreditEntryRef,
        expected: i128,
        got: i128,
    },

    /// A quantity string could not be parsed.
    #[error("invalid quantity: {0}")]
    InvalidQty(String),

    /// An account path is invalid.
    #[error("invalid account path: {0}")]
    InvalidAccount(String),

    /// An asset was re-registered with a different definition.
    #[error("asset conflict for '{name}': existing {existing}, incoming {incoming}")]
    AssetConflict {
        name: String,
        existing: String,
        incoming: String,
    },

    /// Storage backend error.
    #[error("storage error: {0}")]
    Storage(String),
}
