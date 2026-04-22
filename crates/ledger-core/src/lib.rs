//! # Ledger Core
//!
//! Low-level append-only UTXO ledger engine for modeling the movement
//! of value — inventory, cash, receivables, and debt — using spending tokens
//! and hierarchical account paths.

mod account;
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

pub use account::AccountPath;
pub use amount::Amount;
pub use asset::{Asset, AssetKind};
pub use error::LedgerError;
pub use ledger::Ledger;
pub use storage::{MemoryStorage, Storage};
pub use token::{BalanceEntry, EntryRef, SpendingToken, TokenStatus};
pub use transaction::{Credit, DebitRef, Transaction, TransactionBuilder};
