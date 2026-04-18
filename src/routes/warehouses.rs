use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::error::AppError;
use crate::models::product::*;
use crate::state::AppState;

pub async fn list_warehouses(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Warehouse>>, AppError> {
    let warehouses =
        sqlx::query_as::<_, Warehouse>("SELECT * FROM warehouses ORDER BY sort_order, name")
            .fetch_all(&state.pool)
            .await?;
    Ok(Json(warehouses))
}

pub async fn create_warehouse(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWarehouse>,
) -> Result<Json<Warehouse>, AppError> {
    let max_order: Option<i64> = sqlx::query_scalar("SELECT MAX(sort_order) FROM warehouses")
        .fetch_one(&state.pool)
        .await?;
    let next_order = max_order.unwrap_or(0) + 1;

    let warehouse = sqlx::query_as::<_, Warehouse>(
        "INSERT INTO warehouses (name, address, sort_order) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(&body.name)
    .bind(&body.address)
    .bind(next_order)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(warehouse))
}

pub async fn update_warehouse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<CreateWarehouse>,
) -> Result<Json<Warehouse>, AppError> {
    let warehouse = sqlx::query_as::<_, Warehouse>(
        "UPDATE warehouses SET name = ?, address = ? WHERE id = ? RETURNING *",
    )
    .bind(&body.name)
    .bind(&body.address)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Warehouse not found".into()))?;
    Ok(Json(warehouse))
}

pub async fn reorder_warehouses(
    State(state): State<Arc<AppState>>,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<Vec<Warehouse>>, AppError> {
    let mut tx = state.pool.begin().await?;
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE warehouses SET sort_order = ? WHERE id = ?")
            .bind(i as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    list_warehouses(State(state.clone())).await
}
