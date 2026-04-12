use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::inventory::*;

#[derive(Deserialize)]
pub struct StockQuery {
    pub product_id: Option<i64>,
    pub warehouse_id: Option<i64>,
}

#[derive(Deserialize)]
pub struct UtxoQuery {
    pub product_id: Option<i64>,
    pub warehouse_id: Option<i64>,
    pub unspent_only: Option<bool>,
}

pub async fn receive_inventory(
    State(pool): State<SqlitePool>,
    Json(body): Json<ReceiveInventoryRequest>,
) -> Result<Json<InventoryReceipt>, AppError> {
    let mut tx = pool.begin().await?;

    let receipt = sqlx::query_as::<_, InventoryReceipt>(
        "INSERT INTO inventory_receipts (reference, supplier_name, notes)
         VALUES (?, ?, ?) RETURNING *",
    )
    .bind(&body.reference)
    .bind(&body.supplier_name)
    .bind(&body.notes)
    .fetch_one(&mut *tx)
    .await?;

    for line in &body.lines {
        if line.quantity <= 0.0 {
            return Err(AppError::BadRequest("Quantity must be positive".into()));
        }

        // Create UTXO
        sqlx::query(
            "INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(line.product_id)
        .bind(line.warehouse_id)
        .bind(line.quantity)
        .bind(line.cost_per_unit)
        .bind(receipt.id)
        .execute(&mut *tx)
        .await?;

        // Store prices for each customer group
        for price in &line.prices {
            sqlx::query(
                "INSERT INTO inventory_receipt_prices (receipt_id, product_id, customer_group_id, price_per_unit)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(receipt.id)
            .bind(line.product_id)
            .bind(price.customer_group_id)
            .bind(price.price_per_unit)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(Json(receipt))
}

pub async fn get_stock(
    State(pool): State<SqlitePool>,
    Query(params): Query<StockQuery>,
) -> Result<Json<Vec<StockLevel>>, AppError> {
    let stock = if let Some(pid) = params.product_id {
        if let Some(wid) = params.warehouse_id {
            sqlx::query_as::<_, StockLevel>(
                "SELECT product_id, warehouse_id, SUM(quantity) as total_quantity
                 FROM inventory_utxos WHERE spent = 0 AND product_id = ? AND warehouse_id = ?
                 GROUP BY product_id, warehouse_id",
            )
            .bind(pid)
            .bind(wid)
            .fetch_all(&pool)
            .await?
        } else {
            sqlx::query_as::<_, StockLevel>(
                "SELECT product_id, warehouse_id, SUM(quantity) as total_quantity
                 FROM inventory_utxos WHERE spent = 0 AND product_id = ?
                 GROUP BY product_id, warehouse_id",
            )
            .bind(pid)
            .fetch_all(&pool)
            .await?
        }
    } else {
        sqlx::query_as::<_, StockLevel>(
            "SELECT product_id, warehouse_id, SUM(quantity) as total_quantity
             FROM inventory_utxos WHERE spent = 0
             GROUP BY product_id, warehouse_id",
        )
        .fetch_all(&pool)
        .await?
    };
    Ok(Json(stock))
}

pub async fn list_receipts(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<InventoryReceipt>>, AppError> {
    let receipts = sqlx::query_as::<_, InventoryReceipt>(
        "SELECT * FROM inventory_receipts ORDER BY received_at DESC",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(receipts))
}

pub async fn get_receipt(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let receipt = sqlx::query_as::<_, InventoryReceipt>(
        "SELECT * FROM inventory_receipts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Receipt not found".into()))?;

    let utxos = sqlx::query_as::<_, InventoryUtxo>(
        "SELECT * FROM inventory_utxos WHERE receipt_id = ?",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;

    let prices = sqlx::query_as::<_, ReceiptPrice>(
        "SELECT * FROM inventory_receipt_prices WHERE receipt_id = ?",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(serde_json::json!({
        "receipt": receipt,
        "utxos": utxos,
        "prices": prices,
    })))
}

pub async fn list_utxos(
    State(pool): State<SqlitePool>,
    Query(params): Query<UtxoQuery>,
) -> Result<Json<Vec<InventoryUtxo>>, AppError> {
    let unspent_only = params.unspent_only.unwrap_or(true);

    let mut sql = String::from("SELECT * FROM inventory_utxos WHERE 1=1");
    if unspent_only {
        sql.push_str(" AND spent = 0");
    }
    if params.product_id.is_some() {
        sql.push_str(" AND product_id = ?");
    }
    if params.warehouse_id.is_some() {
        sql.push_str(" AND warehouse_id = ?");
    }
    sql.push_str(" ORDER BY created_at ASC");

    let mut query = sqlx::query_as::<_, InventoryUtxo>(&sql);
    if let Some(pid) = params.product_id {
        query = query.bind(pid);
    }
    if let Some(wid) = params.warehouse_id {
        query = query.bind(wid);
    }

    let utxos = query.fetch_all(&pool).await?;
    Ok(Json(utxos))
}

pub async fn product_history(
    State(pool): State<SqlitePool>,
    Path(product_id): Path<i64>,
) -> Result<Json<Vec<InventoryUtxo>>, AppError> {
    let utxos = sqlx::query_as::<_, InventoryUtxo>(
        "SELECT * FROM inventory_utxos WHERE product_id = ? ORDER BY created_at ASC",
    )
    .bind(product_id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(utxos))
}
