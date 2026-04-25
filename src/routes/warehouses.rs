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
    let warehouses = state.db.list_warehouses().await?;
    Ok(Json(warehouses))
}

pub async fn create_warehouse(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWarehouse>,
) -> Result<Json<Warehouse>, AppError> {
    let mut tx = state.db.begin().await?;
    let warehouse = tx.create_warehouse(&body).await?;
    tx.commit().await?;
    Ok(Json(warehouse))
}

pub async fn update_warehouse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<CreateWarehouse>,
) -> Result<Json<Warehouse>, AppError> {
    let mut tx = state.db.begin().await?;
    let warehouse = tx.update_warehouse(id, &body).await?;
    tx.commit().await?;
    Ok(Json(warehouse))
}

pub async fn reorder_warehouses(
    State(state): State<Arc<AppState>>,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<Vec<Warehouse>>, AppError> {
    let mut tx = state.db.begin().await?;
    tx.reorder_warehouses(&ids).await?;
    tx.commit().await?;
    list_warehouses(State(state.clone())).await
}
