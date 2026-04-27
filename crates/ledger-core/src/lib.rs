//! # Ledger Core
//!
//! Low-level append-only UTXO ledger engine for modeling the movement
//! of value — inventory, cash, receivables, and debt — using credit tokens
//! and hierarchical accounts.

mod account;
mod amount;
mod asset;
mod error;
mod ledger;
pub(crate) mod saga;
#[cfg(any(test, feature = "test-support"))]
pub mod storage;
#[cfg(not(any(test, feature = "test-support")))]
mod storage;
mod credit_token;
mod transaction;

pub use account::is_prefix_of;
pub use amount::Amount;
pub use asset::Asset;
pub use credit_token::{CreditEntryRef, CreditToken, CreditTokenStatus};
pub use error::LedgerError;
pub use ledger::Ledger;
pub use storage::{MemoryStorage, Storage};
pub use transaction::{Credit, DebitRef, NetMovement, Transaction, TransactionBuilder};
