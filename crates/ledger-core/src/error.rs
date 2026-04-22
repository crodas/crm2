//! Error types for ledger validation.

use crate::token::EntryRef;

/// All errors that can occur during ledger operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum LedgerError {
    /// A debit references a spending token that does not exist.
    #[error("debit {0} not found in ledger")]
    DebitNotFound(EntryRef),

    /// A debit references a spending token that has already been spent.
    #[error("debit {0} has already been spent")]
    AlreadySpent(EntryRef),

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

    /// An unsigned asset has a negative quantity in a credit.
    #[error("unsigned asset '{asset}' cannot have negative quantity: {qty}")]
    NegativeUnsigned { asset: String, qty: i128 },

    /// A negative monetary credit exists without a matching positive credit
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
        entry_ref: EntryRef,
        expected: String,
        got: String,
    },

    /// The debit's asset does not match the token it references.
    #[error("debit asset mismatch at {entry_ref}: expected {expected}, got {got}")]
    DebitAssetMismatch {
        entry_ref: EntryRef,
        expected: String,
        got: String,
    },

    /// The debit's quantity does not match the token it references.
    #[error("debit qty mismatch at {entry_ref}: expected {expected}, got {got}")]
    DebitQtyMismatch {
        entry_ref: EntryRef,
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
