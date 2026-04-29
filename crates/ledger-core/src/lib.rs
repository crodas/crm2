//! # Ledger Core
//!
//! Low-level append-only UTXO ledger engine for modeling the movement
//! of value — inventory, cash, receivables, and debt — using spending tokens
//! and hierarchical accounts.

mod account;
mod alias;
mod amount;
mod asset;
mod error;
mod ledger;
#[cfg(any(test, feature = "test-support"))]
pub mod storage;
#[cfg(not(any(test, feature = "test-support")))]
mod storage;
mod token;
mod transaction;

pub use alias::{AliasError, AliasMatcher, AliasRegistry, Match as AliasMatch};
pub use amount::Amount;
pub use asset::Asset;
pub use error::LedgerError;
pub use ledger::Ledger;
pub use storage::{MemoryStorage, Storage};
pub use token::{EntryRef, SpendingToken, TokenStatus};
pub use transaction::{Credit, DebitRef, NetMovement, Transaction, TransactionBuilder};
