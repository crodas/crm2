//! SQLite storage backend for [`ledger_core`].
//!
//! Uses sqlx with an embedded migration.

use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{Acquire, Row};

use ledger_core::{
    Asset, BalanceEntry, EntryRef, LedgerError, SpendingToken, Storage, TokenStatus, Transaction,
};

const MIGRATION: &str = include_str!("../migrations/001_ledger.sql");

/// SQLite-backed ledger storage.
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl std::fmt::Debug for SqliteStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStorage").finish_non_exhaustive()
    }
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
    pub async fn from_pool(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        sqlx::query(MIGRATION).execute(&pool).await?;
        Ok(Self { pool })
    }
}

fn db_err(e: sqlx::Error) -> LedgerError {
    LedgerError::Storage(e.to_string())
}

/// Build an Asset from a row that has asset_name and precision columns.
fn asset_from_row(row: &sqlx::sqlite::SqliteRow) -> Asset {
    let name: String = row.get("asset_name");
    let precision: i32 = row.get("precision");
    Asset::new(name, precision as u8)
}

/// SQL fragment that joins ledger_tokens with ledger_assets.
const TOKEN_SELECT: &str =
    "SELECT t.tx_id, t.entry_index, t.owner, t.asset_name, t.qty, t.spent_by_tx,
            a.precision
     FROM ledger_tokens t
     JOIN ledger_assets a ON a.name = t.asset_name";

#[async_trait]
impl Storage for SqliteStorage {
    async fn register_asset(&self, asset: &Asset) -> Result<(), LedgerError> {
        let existing = sqlx::query("SELECT precision FROM ledger_assets WHERE name = ?")
            .bind(asset.name())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;

        if let Some(row) = existing {
            let precision: i32 = row.get("precision");
            if precision == asset.precision() as i32 {
                return Ok(());
            }
            return Err(LedgerError::AssetConflict {
                name: asset.name().to_string(),
                existing: format!("precision={precision}"),
                incoming: format!("precision={}", asset.precision()),
            });
        }

        sqlx::query("INSERT INTO ledger_assets (name, precision, kind) VALUES (?, ?, 'signed')")
            .bind(asset.name())
            .bind(asset.precision() as i32)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn load_assets(&self) -> Result<HashMap<String, Asset>, LedgerError> {
        let rows = sqlx::query("SELECT name, precision FROM ledger_assets")
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        let mut assets = HashMap::new();
        for row in rows {
            let name: String = row.get("name");
            let precision: i32 = row.get("precision");
            assets.insert(name.clone(), Asset::new(name, precision as u8));
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
        let sql = format!("{TOKEN_SELECT} WHERE t.tx_id = ? AND t.entry_index = ?");
        let row = sqlx::query(&sql)
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
                let asset = asset_from_row(&row);
                let qty = row.get::<i64, _>("qty") as i128;
                Ok(Some(SpendingToken {
                    entry_ref: eref.clone(),
                    owner,
                    amount: asset.amount_unchecked(qty),
                    status,
                }))
            }
        }
    }

    async fn unspent_by_account(
        &self,
        account: &str,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let sql = format!(
            "{TOKEN_SELECT} WHERE t.owner = ? AND t.asset_name = ? AND t.spent_by_tx IS NULL"
        );
        let rows = sqlx::query(&sql)
            .bind(account)
            .bind(asset_name)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        rows_to_tokens(rows)
    }

    async fn unspent_by_prefix(
        &self,
        prefix: &str,
        asset_name: &str,
    ) -> Result<Vec<SpendingToken>, LedgerError> {
        let like_pattern = format!("{prefix}/%");

        let sql = format!(
            "{TOKEN_SELECT} WHERE (t.owner = ? OR t.owner LIKE ?)
               AND t.asset_name = ?
               AND t.spent_by_tx IS NULL"
        );
        let rows = sqlx::query(&sql)
            .bind(prefix)
            .bind(&like_pattern)
            .bind(asset_name)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        rows_to_tokens(rows)
    }

    async fn unspent_all_by_prefix(&self, prefix: &str) -> Result<Vec<SpendingToken>, LedgerError> {
        let like_pattern = format!("{prefix}/%");

        let sql = format!(
            "{TOKEN_SELECT} WHERE (t.owner = ? OR t.owner LIKE ?)
               AND t.spent_by_tx IS NULL"
        );
        let rows = sqlx::query(&sql)
            .bind(prefix)
            .bind(&like_pattern)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;

        rows_to_tokens(rows)
    }

    async fn balances_by_prefix(&self, prefix: &str) -> Result<Vec<BalanceEntry>, LedgerError> {
        let like_pattern = format!("{prefix}/%");

        let rows = sqlx::query(
            "SELECT t.owner, t.asset_name, SUM(t.qty) as balance,
                    a.precision
             FROM ledger_tokens t
             JOIN ledger_assets a ON a.name = t.asset_name
             WHERE (t.owner = ? OR t.owner LIKE ?)
               AND t.spent_by_tx IS NULL
             GROUP BY t.owner, t.asset_name
             HAVING SUM(t.qty) != 0
             ORDER BY t.owner, t.asset_name",
        )
        .bind(prefix)
        .bind(&like_pattern)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        rows.into_iter()
            .map(|row| {
                let owner: String = row.get("owner");
                let balance: i64 = row.get("balance");
                let asset = asset_from_row(&row);
                Ok(BalanceEntry {
                    account: owner,
                    amount: asset.amount_unchecked(balance as i128),
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
            .bind(&token.owner)
            .bind(token.amount.asset_name())
            .bind(token.amount.raw() as i64)
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
            let asset = asset_from_row(&row);
            let qty = row.get::<i64, _>("qty") as i128;
            Ok(SpendingToken {
                entry_ref: EntryRef {
                    tx_id,
                    entry_index: entry_index as u32,
                },
                owner,
                amount: asset.amount_unchecked(qty),
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

        use ledger_core::{Asset, Storage};
        let brush = Asset::new("brush", 0);
        storage
            .register_asset(&brush)
            .await
            .expect("register_asset");

        let assets = storage.load_assets().await.expect("load_assets");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets["brush"], brush);
    }
}
