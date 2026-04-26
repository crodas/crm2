use super::{Db, Tx};
use crate::error::AppError;

impl Db {
    pub async fn get_config(&self) -> Result<Vec<(String, String)>, AppError> {
        let rows = sqlx::query_as("SELECT key, value FROM config")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }
}

impl Tx {
    pub async fn set_config(&mut self, key: &str, value: &str) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO config (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *self.inner)
        .await?;
        Ok(())
    }
}
