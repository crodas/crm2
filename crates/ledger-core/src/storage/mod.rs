//! Async storage trait and in-memory implementation.
//!
//! The [`Storage`] trait abstracts persistence so the ledger can run against
//! any backend (SQLite, Postgres, in-memory, etc.). All operations are async
//! to support database-backed implementations.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use async_trait::async_trait;

use crate::account::AccountPath;
use crate::asset::Asset;
use crate::error::LedgerError;
use crate::token::{EntryRef, SpendingToken, TokenStatus};
use crate::transaction::Transaction;

/// Async storage backend for the ledger.
///
/// Implementations must guarantee atomicity: if `commit_tx` succeeds, all
/// writes (transaction, tokens, spent marks, idempotency key) are durable.
/// If it fails, none are applied.
#[async_trait]
pub trait Storage: Send + Sync {
    // ── Assets ─────────────────────────────────────────────────────

    /// Persist an asset definition.
    ///
    /// If an asset with the same name already exists and is identical, this
    /// is a no-op. If the existing asset differs (e.g. different precision
    /// or kind), implementations must return [`LedgerError::AssetConflict`].
    async fn register_asset(&self, asset: &Asset) -> Result<(), LedgerError>;

    /// Load all registered assets, keyed by name.
    async fn load_assets(&self) -> Result<HashMap<String, Asset>, LedgerError>;

    // ── Idempotency ────────────────────────────────────────────────

    /// Return `true` if this idempotency key has already been committed.
    async fn has_idempotency_key(&self, key: &str) -> Result<bool, LedgerError>;

    // ── Tokens ─────────────────────────────────────────────────────

    /// Fetch a single spending token by its entry reference.
    ///
    /// Returns `None` if no token exists at that reference.
    async fn get_token(&self, eref: &EntryRef) -> Result<Option<SpendingToken>, LedgerError>;

    /// Return all unspent tokens owned by `account` for the given asset.
    async fn unspent_by_account(
        &self,
        account: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError>;

    /// Return all unspent tokens under `prefix` for the given asset.
    async fn unspent_by_prefix(
        &self,
        prefix: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError>;

    // ── Transactions ───────────────────────────────────────────────

    /// Atomically commit a transaction to storage.
    ///
    /// This must persist in a single atomic operation:
    /// - The transaction itself
    /// - New spending tokens (one per credit)
    /// - Mark consumed tokens as spent (one per debit)
    /// - The idempotency key
    ///
    /// If any step fails, the entire operation must be rolled back.
    async fn commit_tx(
        &self,
        tx: &Transaction,
        new_tokens: &[SpendingToken],
        spent_refs: &[EntryRef],
    ) -> Result<(), LedgerError>;

    /// Load all transactions in append order.
    async fn load_transactions(&self) -> Result<Vec<Transaction>, LedgerError>;

    /// Return the total number of committed transactions.
    async fn tx_count(&self) -> Result<usize, LedgerError>;
}

// ── In-memory implementation ───────────────────────────────────────

struct MemoryState {
    assets: HashMap<String, Asset>,
    transactions: Vec<Transaction>,
    tokens: HashMap<EntryRef, SpendingToken>,
    idempotency_keys: HashSet<String>,
}

/// In-memory storage backend.
///
/// All data lives in a `RwLock`-protected struct. Useful for testing and
/// single-process deployments where durability is not required.
pub struct MemoryStorage {
    state: RwLock<MemoryState>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(MemoryState {
                assets: HashMap::new(),
                transactions: Vec::new(),
                tokens: HashMap::new(),
                idempotency_keys: HashSet::new(),
            }),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

fn lock_err(e: impl std::fmt::Display) -> LedgerError {
    LedgerError::Storage(format!("lock poisoned: {e}"))
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn register_asset(&self, asset: &Asset) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;
        if let Some(existing) = state.assets.get(asset.name()) {
            if existing == asset {
                return Ok(());
            }
            return Err(LedgerError::AssetConflict {
                name: asset.name().to_string(),
                existing: format!("{existing:?}"),
                incoming: format!("{asset:?}"),
            });
        }
        state.assets.insert(asset.name().to_string(), asset.clone());
        Ok(())
    }

    async fn load_assets(&self) -> Result<HashMap<String, Asset>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state.assets.clone())
    }

    async fn has_idempotency_key(&self, key: &str) -> Result<bool, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state.idempotency_keys.contains(key))
    }

    async fn get_token(&self, eref: &EntryRef) -> Result<Option<SpendingToken>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state.tokens.get(eref).cloned())
    }

    async fn unspent_by_account(
        &self,
        account: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state
            .tokens
            .values()
            .filter(|t| {
                t.status == TokenStatus::Unspent
                    && t.owner == *account
                    && t.asset_name == asset_name
            })
            .cloned()
            .collect())
    }

    async fn unspent_by_prefix(
        &self,
        prefix: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state
            .tokens
            .values()
            .filter(|t| {
                t.status == TokenStatus::Unspent
                    && prefix.is_prefix_of(&t.owner)
                    && t.asset_name == asset_name
            })
            .cloned()
            .collect())
    }

    async fn commit_tx(
        &self,
        tx: &Transaction,
        new_tokens: &[SpendingToken],
        spent_refs: &[EntryRef],
    ) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;

        // Mark spent tokens.
        let tx_index = state.transactions.len();
        for eref in spent_refs {
            if let Some(token) = state.tokens.get_mut(eref) {
                token.status = TokenStatus::Spent(tx_index);
            }
        }

        // Insert new tokens.
        for token in new_tokens {
            state.tokens.insert(token.entry_ref.clone(), token.clone());
        }

        state.idempotency_keys.insert(tx.idempotency_key.clone());
        state.transactions.push(tx.clone());
        Ok(())
    }

    async fn load_transactions(&self) -> Result<Vec<Transaction>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state.transactions.clone())
    }

    async fn tx_count(&self) -> Result<usize, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state.transactions.len())
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;
