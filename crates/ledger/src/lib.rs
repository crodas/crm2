//! # Ledger
//!
//! High-level UTXO ledger with automatic credit token selection and debt operations.
//!
//! Re-exports core types from [`ledger_core`] and adds:
//! - [`TransactionBuilder`] — automatic credit token selection for debits, plus
//!   [`issue`](TransactionBuilder::issue),
//!   [`create_debt`](TransactionBuilder::create_debt), and
//!   [`settle_debt`](TransactionBuilder::settle_debt)
//! - [`Ledger`] — wraps the core ledger with `.transaction()` method
//!
//! For low-level access (explicit entry refs), use [`ledger_core`] directly.

mod builder;
pub mod debt;
mod error;
pub mod issuance;
mod ledger;

// Expose ledger_core as a sub-module.
pub use ledger_core;

// Re-export core types for convenience.
pub use ledger_core::{
    Amount, Asset, BalanceEntry, Credit, CreditEntryRef, CreditToken, DebitRef, LedgerError,
    MemoryStorage, NetMovement, Storage, CreditTokenStatus, Transaction,
};

// High-level API.
pub use builder::TransactionBuilder;
pub use error::Error;
pub use ledger::Ledger;
