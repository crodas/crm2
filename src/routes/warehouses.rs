use axum::{extract::State, Json};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::product::*;

pub async fn list_warehouses(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<Warehouse>>, AppError> {
    let warehouses = sqlx::query_as::<_, Warehouse>("SELECT * FROM warehouses ORDER BY name")
        .fetch_all(&pool)
        .await?;
    Ok(Json(warehouses))
}

pub async fn create_warehouse(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateWarehouse>,
) -> Result<Json<Warehouse>, AppError> {
    let warehouse = sqlx::query_as::<_, Warehouse>(
        "INSERT INTO warehouses (name, address) VALUES (?, ?) RETURNING *",
    )
    .bind(&body.name)
    .bind(&body.address)
    .fetch_one(&pool)
    .await?;
    Ok(Json(warehouse))
}
