//! # Ledger
//!
//! High-level UTXO ledger with automatic token selection and debt operations.
//!
//! Re-exports core types from [`ledger_core`] and adds:
//! - [`TransactionBuilder`] — automatic token selection for debits, plus
//!   [`create_debt`](TransactionBuilder::create_debt) and
//!   [`settle_debt`](TransactionBuilder::settle_debt) when a
//!   [`DebtStrategy`](debt::DebtStrategy) is configured
//! - [`Ledger`] — wraps the core ledger with `.transaction()` method
//!
//! For low-level access (explicit entry refs), use [`ledger_core`] directly.

mod builder;
pub mod credit;
pub mod debt;
mod error;
mod ledger;

// Expose ledger_core as a sub-module.
pub use ledger_core;

// Re-export core types for convenience.
pub use ledger_core::{
    Amount, Asset, AssetKind, BalanceEntry, Credit, DebitRef, EntryRef, LedgerError, MemoryStorage,
    SpendingToken, Storage, TokenStatus, Transaction,
};

// High-level API.
pub use builder::TransactionBuilder;
pub use error::Error;
pub use ledger::Ledger;
