use axum::{extract::State, Json};
use serde_json::{Map, Value};
use sqlx::SqlitePool;

use crate::error::AppError;

pub async fn get_config(State(pool): State<SqlitePool>) -> Result<Json<Value>, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM config")
        .fetch_all(&pool)
        .await?;

    let mut map = Map::new();
    for (key, value) in rows {
        // Try to parse as JSON first, fallback to string
        let v = serde_json::from_str(&value).unwrap_or(Value::String(value));
        map.insert(key, v);
    }
    Ok(Json(Value::Object(map)))
}

pub async fn update_config(
    State(pool): State<SqlitePool>,
    Json(body): Json<Map<String, Value>>,
) -> Result<Json<Value>, AppError> {
    for (key, value) in &body {
        let val_str = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        sqlx::query(
            "INSERT INTO config (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        )
        .bind(key)
        .bind(&val_str)
        .execute(&pool)
        .await?;
    }
    get_config(State(pool)).await
}
