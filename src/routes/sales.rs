use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::inventory::InventoryUtxo;
use crate::models::sale::*;

pub async fn create_sale(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateSaleRequest>,
) -> Result<Json<Sale>, AppError> {
    let mut tx = pool.begin().await?;

    // Calculate total
    let total: f64 = body
        .lines
        .iter()
        .map(|l| l.quantity * l.price_per_unit)
        .sum();

    let sale = sqlx::query_as::<_, Sale>(
        "INSERT INTO sales (customer_id, customer_group_id, notes, total_amount)
         VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(body.customer_id)
    .bind(body.customer_group_id)
    .bind(&body.notes)
    .bind(total)
    .fetch_one(&mut *tx)
    .await?;

    for line in &body.lines {
        if line.quantity <= 0.0 {
            return Err(AppError::BadRequest("Quantity must be positive".into()));
        }

        // Insert sale line
        let sale_line = sqlx::query_as::<_, SaleLine>(
            "INSERT INTO sale_lines (sale_id, product_id, quantity, price_per_unit)
             VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(sale.id)
        .bind(line.product_id)
        .bind(line.quantity)
        .bind(line.price_per_unit)
        .fetch_one(&mut *tx)
        .await?;

        // Get unspent UTXOs for this product+warehouse (FIFO)
        let utxos = sqlx::query_as::<_, InventoryUtxo>(
            "SELECT * FROM inventory_utxos
             WHERE product_id = ? AND warehouse_id = ? AND spent = 0
             ORDER BY created_at ASC",
        )
        .bind(line.product_id)
        .bind(line.warehouse_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut remaining = line.quantity;

        for utxo in &utxos {
            if remaining <= 0.0 {
                break;
            }

            let used = remaining.min(utxo.quantity);
            remaining -= used;

            // Mark UTXO as spent
            sqlx::query(
                "UPDATE inventory_utxos SET spent = 1, spent_by_sale_id = ? WHERE id = ?",
            )
            .bind(sale.id)
            .bind(utxo.id)
            .execute(&mut *tx)
            .await?;

            // Record which UTXOs were consumed
            sqlx::query(
                "INSERT INTO sale_line_utxo_inputs (sale_line_id, utxo_id, quantity_used)
                 VALUES (?, ?, ?)",
            )
            .bind(sale_line.id)
            .bind(utxo.id)
            .bind(used)
            .execute(&mut *tx)
            .await?;

            // Create change UTXO if partial consumption
            if used < utxo.quantity {
                let change = utxo.quantity - used;
                sqlx::query(
                    "INSERT INTO inventory_utxos
                     (product_id, warehouse_id, quantity, cost_per_unit, source_sale_id, spent)
                     VALUES (?, ?, ?, ?, ?, 0)",
                )
                .bind(line.product_id)
                .bind(line.warehouse_id)
                .bind(change)
                .bind(utxo.cost_per_unit)
                .bind(sale.id)
                .execute(&mut *tx)
                .await?;
            }
        }

        if remaining > 0.0 {
            let available = line.quantity - remaining;
            return Err(AppError::InsufficientStock {
                product_id: line.product_id,
                requested: line.quantity,
                available,
            });
        }
    }

    tx.commit().await?;
    Ok(Json(sale))
}

pub async fn list_sales(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<Sale>>, AppError> {
    let sales = sqlx::query_as::<_, Sale>("SELECT * FROM sales ORDER BY sold_at DESC")
        .fetch_all(&pool)
        .await?;
    Ok(Json(sales))
}

pub async fn get_sale(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sale = sqlx::query_as::<_, Sale>("SELECT * FROM sales WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Sale not found".into()))?;

    let lines = sqlx::query_as::<_, SaleLine>("SELECT * FROM sale_lines WHERE sale_id = ?")
        .bind(id)
        .fetch_all(&pool)
        .await?;

    Ok(Json(serde_json::json!({
        "sale": sale,
        "lines": lines,
    })))
}
