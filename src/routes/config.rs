use axum::{extract::State, Json};
use serde_json::{Map, Value};
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;

pub async fn get_config(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let rows = state.db.get_config().await?;
    let mut map = Map::new();
    for (key, value) in rows {
        let v = serde_json::from_str(&value).unwrap_or_else(|_| Value::String(value.clone()));
        map.insert(key, v);
    }
    Ok(Json(Value::Object(map)))
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Map<String, Value>>,
) -> Result<Json<Value>, AppError> {
    let mut tx = state.db.begin().await?;
    for (key, value) in &body {
        let val_str = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        tx.set_config(key, &val_str).await?;
    }
    tx.commit().await?;
    get_config(State(state.clone())).await
}
