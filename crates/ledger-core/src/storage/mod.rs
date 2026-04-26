//! Async storage trait and in-memory implementation.
//!
//! The [`Storage`] trait abstracts persistence so the ledger can run against
//! any backend (SQLite, Postgres, in-memory, etc.). All operations are async
//! to support database-backed implementations.
//!
//! Write operations are granular — the saga layer in [`crate::saga`] composes
//! them into an all-or-nothing commit with automatic compensation on failure.

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::RwLock;

use async_trait::async_trait;

use crate::account::is_prefix_of;
use crate::amount::Amount;
use crate::asset::Asset;
use crate::error::LedgerError;
use crate::token::{BalanceEntry, CreditEntryRef, CreditToken, TokenStatus};
use crate::transaction::Transaction;

/// Async storage backend for the ledger.
///
/// Read methods are used for validation and queries. Write methods are
/// granular primitives composed by the saga layer into atomic commits
/// with compensation on failure.
#[async_trait]
pub trait Storage: Send + Sync + Debug {
    // ── Assets ─────────────────────────────────────────────────────

    /// Persist an asset definition.
    async fn register_asset(&self, asset: &Asset) -> Result<(), LedgerError>;

    /// Load all registered assets, keyed by name.
    async fn load_assets(&self) -> Result<HashMap<String, Asset>, LedgerError>;

    // ── Idempotency ────────────────────────────────────────────────

    /// Return `true` if this idempotency key has already been committed.
    async fn has_idempotency_key(&self, key: &str) -> Result<bool, LedgerError>;

    // ── Tokens ─────────────────────────────────────────────────────

    /// Fetch a single spending token by its entry reference.
    async fn get_token(&self, eref: &CreditEntryRef) -> Result<Option<CreditToken>, LedgerError>;

    /// Return unspent tokens owned by `account`.
    ///
    /// - `Some(amount)` — only tokens matching the amount's asset; errors if
    ///   the available sum is less than `amount.raw()`.
    /// - `None` — all unspent tokens across all assets.
    async fn unspent_by_account(
        &self,
        account: &str,
        requested_amount: Option<&Amount>,
    ) -> Result<Vec<CreditToken>, LedgerError>;

    /// Return unspent tokens under `prefix`.
    ///
    /// - `Some(amount)` — only tokens matching the amount's asset; errors if
    ///   the available sum is less than `amount.raw()`.
    /// - `None` — all unspent tokens across all assets.
    async fn unspent_by_prefix(
        &self,
        prefix: &str,
        requested_amount: Option<&Amount>,
    ) -> Result<Vec<CreditToken>, LedgerError>;

    /// Return aggregated balances grouped by (account, asset) for all
    /// unspent tokens under `prefix`.
    async fn balances_by_prefix(&self, prefix: &str) -> Result<Vec<BalanceEntry>, LedgerError>;

    // ── Granular write primitives ─────────────────────────────────

    /// Mark the given tokens as spent by `by_tx`.
    ///
    /// Each referenced token must exist and be unspent.
    async fn mark_spent(&self, refs: &[CreditEntryRef], by_tx: &str) -> Result<(), LedgerError>;

    /// Compensation: unmark previously-spent tokens back to unspent.
    ///
    /// Only reverts tokens whose `spent_by_tx` matches `tx_to_revert`,
    /// leaving tokens spent by other transactions untouched.
    async fn unmark_spent(
        &self,
        refs: &[CreditEntryRef],
        tx_to_revert: &str,
    ) -> Result<(), LedgerError>;

    /// Insert new spending tokens into the store.
    async fn insert_tokens(&self, tokens: &[CreditToken]) -> Result<(), LedgerError>;

    /// Compensation: remove tokens by their entry references.
    async fn remove_tokens(&self, refs: &[CreditEntryRef]) -> Result<(), LedgerError>;

    /// Insert a committed transaction record and its idempotency key.
    async fn insert_tx(&self, tx: &Transaction) -> Result<(), LedgerError>;

    /// Compensation: remove a transaction record and its idempotency key.
    async fn remove_tx(&self, tx_id: &str) -> Result<(), LedgerError>;

    // ── Transaction queries ──────────────────────────────────────

    /// Load all transactions in append order.
    async fn load_transactions(&self) -> Result<Vec<Transaction>, LedgerError>;

    /// Return the total number of committed transactions.
    async fn tx_count(&self) -> Result<usize, LedgerError>;
}

// ── In-memory implementation ───────────────────────────────────────

#[derive(Debug)]
struct MemoryState {
    assets: HashMap<String, Asset>,
    transactions: Vec<Transaction>,
    tokens: HashMap<CreditEntryRef, CreditToken>,
    idempotency_keys: HashSet<String>,
}

/// In-memory storage backend.
///
/// All data lives in a `RwLock`-protected struct. Useful for testing and
/// single-process deployments where durability is not required.
pub struct MemoryStorage {
    state: RwLock<MemoryState>,
}

impl std::fmt::Debug for MemoryStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryStorage").finish_non_exhaustive()
    }
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

    async fn get_token(&self, eref: &CreditEntryRef) -> Result<Option<CreditToken>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state.tokens.get(eref).cloned())
    }

    async fn unspent_by_account(
        &self,
        account: &str,
        requested_amount: Option<&Amount>,
    ) -> Result<Vec<CreditToken>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state
            .tokens
            .values()
            .filter(|t| {
                t.status == TokenStatus::Unspent
                    && t.owner == account
                    && requested_amount.map_or(true, |a| t.amount.asset_name() == a.asset_name())
            })
            .cloned()
            .collect())
    }

    async fn unspent_by_prefix(
        &self,
        prefix: &str,
        requested_amount: Option<&Amount>,
    ) -> Result<Vec<CreditToken>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        Ok(state
            .tokens
            .values()
            .filter(|t| {
                t.status == TokenStatus::Unspent
                    && is_prefix_of(prefix, &t.owner)
                    && requested_amount.map_or(true, |a| t.amount.asset_name() == a.asset_name())
            })
            .cloned()
            .collect())
    }

    async fn balances_by_prefix(&self, prefix: &str) -> Result<Vec<BalanceEntry>, LedgerError> {
        let state = self.state.read().map_err(lock_err)?;
        let mut map: HashMap<(String, String), (crate::asset::Asset, i128)> = HashMap::new();
        for t in state.tokens.values() {
            if t.status == TokenStatus::Unspent && is_prefix_of(prefix, &t.owner) {
                let key = (t.owner.clone(), t.amount.asset_name().to_string());
                let entry = map
                    .entry(key)
                    .or_insert_with(|| (t.amount.asset().clone(), 0));
                entry.1 += t.amount.raw();
            }
        }
        let mut entries: Vec<BalanceEntry> = map
            .into_iter()
            .filter(|(_, (_, balance))| *balance != 0)
            .map(|((account, _asset_name), (asset, balance))| BalanceEntry {
                account,
                amount: Amount::new_unchecked(asset, balance),
            })
            .collect();
        entries.sort_by(|a, b| {
            a.account
                .cmp(&b.account)
                .then(a.amount.asset_name().cmp(b.amount.asset_name()))
        });
        Ok(entries)
    }

    async fn mark_spent(&self, refs: &[CreditEntryRef], _by_tx: &str) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;
        let tx_index = state.transactions.len();
        for eref in refs {
            if let Some(token) = state.tokens.get_mut(eref) {
                token.status = TokenStatus::Spent(tx_index);
            }
        }
        Ok(())
    }

    async fn unmark_spent(
        &self,
        refs: &[CreditEntryRef],
        _tx_to_revert: &str,
    ) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;
        for eref in refs {
            if let Some(token) = state.tokens.get_mut(eref) {
                token.status = TokenStatus::Unspent;
            }
        }
        Ok(())
    }

    async fn insert_tokens(&self, tokens: &[CreditToken]) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;
        for token in tokens {
            state.tokens.insert(token.entry_ref.clone(), token.clone());
        }
        Ok(())
    }

    async fn remove_tokens(&self, refs: &[CreditEntryRef]) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;
        for eref in refs {
            state.tokens.remove(eref);
        }
        Ok(())
    }

    async fn insert_tx(&self, tx: &Transaction) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;
        state.idempotency_keys.insert(tx.idempotency_key.clone());
        state.transactions.push(tx.clone());
        Ok(())
    }

    async fn remove_tx(&self, tx_id: &str) -> Result<(), LedgerError> {
        let mut state = self.state.write().map_err(lock_err)?;
        if let Some(pos) = state.transactions.iter().position(|t| t.tx_id == tx_id) {
            let removed = state.transactions.remove(pos);
            state.idempotency_keys.remove(&removed.idempotency_key);
        }
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
