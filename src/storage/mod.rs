use std::sync::Arc;

use sqlx::sqlite::{Sqlite, SqlitePool};
use sqlx::Transaction;

use crate::error::AppError;

pub mod bookings;
pub mod config;
pub mod customers;
pub mod inventory;
pub mod payments;
pub mod products;
pub mod quotes;
pub mod sales;
pub mod teams;

/// Database access handle. Cheap to clone (wraps Arc-based pool + ledger).
#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
    ledger: Arc<ledger::Ledger>,
    store_id: String,
}

/// A transaction handle. Auto-rolls-back on drop unless `.commit()` is called.
pub struct Tx {
    inner: Transaction<'static, Sqlite>,
    ledger: Arc<ledger::Ledger>,
    store_id: String,
}

impl Db {
    pub fn new(pool: SqlitePool, ledger: ledger::Ledger, store_id: String) -> Self {
        Self {
            pool,
            ledger: Arc::new(ledger),
            store_id,
        }
    }

    /// Access the underlying CRM pool (for test setup and main.rs init only).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Access the ledger (for test setup only).
    pub fn ledger(&self) -> &Arc<ledger::Ledger> {
        &self.ledger
    }

    pub fn store_id(&self) -> &str {
        &self.store_id
    }

    /// Begin a transaction. If the returned `Tx` is dropped without
    /// calling `.commit()`, all CRM SQL changes are automatically rolled back.
    pub async fn begin(&self) -> Result<Tx, AppError> {
        let tx = self.pool.begin().await?;
        Ok(Tx {
            inner: tx,
            ledger: self.ledger.clone(),
            store_id: self.store_id.clone(),
        })
    }

    /// Look up a registered ledger asset by name.
    pub fn asset(&self, name: &str) -> Option<ledger::Asset> {
        self.ledger.asset(name)
    }

    /// Register a ledger asset (used during initialization).
    pub async fn register_asset(&self, asset: ledger::Asset) -> Result<(), AppError> {
        self.ledger
            .register_asset(asset)
            .await
            .map_err(|e| AppError::Internal(format!("register asset: {e}")))
    }
}

impl Tx {
    /// Commit the transaction. If this is not called, the transaction
    /// rolls back when `Tx` is dropped.
    pub async fn commit(self) -> Result<(), AppError> {
        self.inner.commit().await?;
        Ok(())
    }
}
