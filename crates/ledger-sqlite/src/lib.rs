//! SQLite storage backend for [`ledger_core`].
//!
//! Uses sqlx with an embedded migration.

use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{Acquire, Row};

use ledger_core::{
    AccountPath, Asset, AssetKind, BalanceEntry, EntryRef, LedgerError, SpendingToken, Storage,
    TokenStatus, Transaction,
};

const MIGRATION: &str = include_str!("../migrations/001_ledger.sql");

/// SQLite-backed ledger storage.
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    /// Connect to a SQLite database and run migrations.
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;

        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await?;

        sqlx::query(MIGRATION).execute(&pool).await?;

        Ok(Self { pool })
    }

    /// Create storage from an existing connection pool and run migrations.
    ///
    /// Use this when sharing a pool with the rest of the application so the
    /// ledger tables live in the same database.
    pub async fn from_pool(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        sqlx::query(MIGRATION).execute(&pool).await?;
        Ok(Self { pool })
    }
}

fn db_err(e: sqlx::Error) -> LedgerError {
    LedgerError::Storage(e.to_string())
}

fn kind_to_str(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Signed => "signed",
        AssetKind::Unsigned => "unsigned",
    }
}

fn str_to_kind(s: &str) -> AssetKind {
    match s {
        "signed" => AssetKind::Signed,
        _ => AssetKind::Unsigned,
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn register_asset(&self, asset: &Asset) -> Result<(), LedgerError> {
        let existing = sqlx::query("SELECT precision, kind FROM ledger_assets WHERE name = ?")
            .bind(asset.name())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;

        if let Some(row) = existing {
            let precision: i32 = row.get("precision");
            let kind: String = row.get("kind");
            if precision == asset.precision() as i32 && kind == kind_to_str(asset.kind()) {
                return Ok(());
            }
            return Err(LedgerError::AssetConflict {
                name: asset.name().to_string(),
                existing: format!("precision={precision}, kind={kind}"),
                incoming: format!(
                    "precision={}, kind={}",
                    asset.precision(),
                    kind_to_str(asset.kind())
                ),
            });
        }

        sqlx::query("INSERT INTO ledger_assets (name, precision, kind) VALUES (?, ?, ?)")
            .bind(asset.name())
            .bind(asset.precision() as i32)
            .bind(kind_to_str(asset.kind()))
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn load_assets(&self) -> Result<HashMap<String, Asset>, LedgerError> {
        let rows = sqlx::query("SELECT name, precision, kind FROM ledger_assets")
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        let mut assets = HashMap::new();
        for row in rows {
            let name: String = row.get("name");
            let precision: i32 = row.get("precision");
            let kind: String = row.get("kind");
            assets.insert(
                name.clone(),
                Asset::new(name, precision as u8, str_to_kind(&kind)),
            );
        }
        Ok(assets)
    }

    async fn has_idempotency_key(&self, key: &str) -> Result<bool, LedgerError> {
        let row = sqlx::query("SELECT 1 FROM ledger_transactions WHERE idempotency_key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(row.is_some())
    }

    async fn get_token(&self, eref: &EntryRef) -> Result<Option<SpendingToken>, LedgerError> {
        let row = sqlx::query(
            "SELECT tx_id, entry_index, owner, asset_name, qty, spent_by_tx
             FROM ledger_tokens WHERE tx_id = ? AND entry_index = ?",
        )
        .bind(&eref.tx_id)
        .bind(eref.entry_index as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        match row {
            None => Ok(None),
            Some(row) => {
                let owner: String = row.get("owner");
                let spent_by_tx: Option<String> = row.get("spent_by_tx");
                let status = match spent_by_tx {
                    None => TokenStatus::Unspent,
                    Some(_) => TokenStatus::Spent(0),
                };
                Ok(Some(SpendingToken {
                    entry_ref: eref.clone(),
                    owner: AccountPath::new(owner)
                        .map_err(|e| LedgerError::InvalidAccount(e.to_string()))?,
                    asset_name: row.get("asset_name"),
                    qty: row.get::<i64, _>("qty") as i128,
                    status,
                }))
            }
        }
    }

    async fn unspent_by_account(
        &self,
        account: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let rows = sqlx::query(
            "SELECT tx_id, entry_index, owner, asset_name, qty
             FROM ledger_tokens
             WHERE owner = ? AND asset_name = ? AND spent_by_tx IS NULL",
        )
        .bind(account.as_str())
        .bind(asset_name)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows_to_tokens(rows)
    }

    async fn unspent_by_prefix(
        &self,
        prefix: &AccountPath,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let prefix_str = prefix.as_str();
        let like_pattern = format!("{prefix_str}/%");

        let rows = sqlx::query(
            "SELECT tx_id, entry_index, owner, asset_name, qty
             FROM ledger_tokens
             WHERE (owner = ? OR owner LIKE ?)
               AND asset_name = ?
               AND spent_by_tx IS NULL",
        )
        .bind(prefix_str)
        .bind(&like_pattern)
        .bind(asset_name)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows_to_tokens(rows)
    }

    async fn unspent_all_by_prefix(
        &self,
        prefix: &AccountPath,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let prefix_str = prefix.as_str();
        let like_pattern = format!("{prefix_str}/%");

        let rows = sqlx::query(
            "SELECT tx_id, entry_index, owner, asset_name, qty
             FROM ledger_tokens
             WHERE (owner = ? OR owner LIKE ?)
               AND spent_by_tx IS NULL",
        )
        .bind(prefix_str)
        .bind(&like_pattern)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows_to_tokens(rows)
    }

    async fn balances_by_prefix(
        &self,
        prefix: &AccountPath,
    ) -> Result<Vec<BalanceEntry>, LedgerError> {
        let prefix_str = prefix.as_str();
        let like_pattern = format!("{prefix_str}/%");

        let rows = sqlx::query(
            "SELECT owner, asset_name, SUM(qty) as balance
             FROM ledger_tokens
             WHERE (owner = ? OR owner LIKE ?)
               AND spent_by_tx IS NULL
             GROUP BY owner, asset_name
             HAVING SUM(qty) != 0
             ORDER BY owner, asset_name",
        )
        .bind(prefix_str)
        .bind(&like_pattern)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.into_iter()
            .map(|row| {
                let owner: String = row.get("owner");
                let balance: i64 = row.get("balance");
                Ok(BalanceEntry {
                    account: AccountPath::new(owner)
                        .map_err(|e| LedgerError::InvalidAccount(e.to_string()))?,
                    asset_name: row.get("asset_name"),
                    balance: balance as i128,
                })
            })
            .collect()
    }

    async fn commit_tx(
        &self,
        tx: &Transaction,
        new_tokens: &[SpendingToken],
        spent_refs: &[EntryRef],
    ) -> Result<(), LedgerError> {
        let data = serde_json::to_string(tx).map_err(|e| LedgerError::Storage(e.to_string()))?;

        let mut conn = self.pool.acquire().await.map_err(db_err)?;
        let mut db_tx = conn.begin().await.map_err(db_err)?;

        sqlx::query(
            "INSERT INTO ledger_transactions (tx_id, idempotency_key, data) VALUES (?, ?, ?)",
        )
        .bind(&tx.tx_id)
        .bind(&tx.idempotency_key)
        .bind(&data)
        .execute(&mut *db_tx)
        .await
        .map_err(db_err)?;

        for token in new_tokens {
            sqlx::query(
                "INSERT INTO ledger_tokens (tx_id, entry_index, owner, asset_name, qty)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&token.entry_ref.tx_id)
            .bind(token.entry_ref.entry_index as i32)
            .bind(token.owner.as_str())
            .bind(&token.asset_name)
            .bind(token.qty as i64)
            .execute(&mut *db_tx)
            .await
            .map_err(db_err)?;
        }

        for eref in spent_refs {
            sqlx::query(
                "UPDATE ledger_tokens SET spent_by_tx = ? WHERE tx_id = ? AND entry_index = ?",
            )
            .bind(&tx.tx_id)
            .bind(&eref.tx_id)
            .bind(eref.entry_index as i32)
            .execute(&mut *db_tx)
            .await
            .map_err(db_err)?;
        }

        db_tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn load_transactions(&self) -> Result<Vec<Transaction>, LedgerError> {
        let rows = sqlx::query("SELECT data FROM ledger_transactions ORDER BY rowid")
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        rows.iter()
            .map(|row| {
                let data: String = row.get("data");
                serde_json::from_str(&data).map_err(|e| LedgerError::Storage(e.to_string()))
            })
            .collect()
    }

    async fn tx_count(&self) -> Result<usize, LedgerError> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM ledger_transactions")
            .fetch_one(&self.pool)
            .await
            .map_err(db_err)?;
        let cnt: i64 = row.get("cnt");
        Ok(cnt as usize)
    }
}

fn rows_to_tokens(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<SpendingToken>, LedgerError> {
    rows.into_iter()
        .map(|row| {
            let tx_id: String = row.get("tx_id");
            let entry_index: i32 = row.get("entry_index");
            let owner: String = row.get("owner");
            Ok(SpendingToken {
                entry_ref: EntryRef {
                    tx_id,
                    entry_index: entry_index as u32,
                },
                owner: AccountPath::new(owner)
                    .map_err(|e| LedgerError::InvalidAccount(e.to_string()))?,
                asset_name: row.get("asset_name"),
                qty: row.get::<i64, _>("qty") as i128,
                status: TokenStatus::Unspent,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::SqliteStorage;
    use ledger_core::storage_tests;

    storage_tests!(async {
        SqliteStorage::connect("sqlite::memory:")
            .await
            .expect("connect")
    });

    #[tokio::test]
    async fn from_pool_works() {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");

        let storage = SqliteStorage::from_pool(pool).await.expect("from_pool");

        // Verify it works by running a basic operation
        use ledger_core::{Asset, AssetKind, Storage};
        let brush = Asset::new("brush", 0, AssetKind::Unsigned);
        storage
            .register_asset(&brush)
            .await
            .expect("register_asset");

        let assets = storage.load_assets().await.expect("load_assets");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets["brush"], brush);
    }
}
