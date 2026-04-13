use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::SqlitePool;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::inventory::InventoryUtxo;
use crate::models::sale::*;

/// Core sale creation logic, usable from both the HTTP handler and tests.
/// Each tuple in `lines` is `(product_id, warehouse_id, quantity, price_per_unit_cents)`.
pub async fn create_sale_tx(
    pool: &SqlitePool,
    customer_id: i64,
    customer_group_id: i64,
    notes: Option<&str>,
    lines: &[(i64, i64, f64, i64)],
) -> Result<Sale, AppError> {
    let mut tx = pool.begin().await?;

    let total: Amount = lines
        .iter()
        .map(|&(_, _, qty, price)| Amount(price).mul_qty(qty))
        .sum();

    let sale = sqlx::query_as::<_, Sale>(
        "INSERT INTO sales (customer_id, customer_group_id, notes, total_amount)
         VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(customer_id)
    .bind(customer_group_id)
    .bind(notes)
    .bind(total)
    .fetch_one(&mut *tx)
    .await?;

    for &(product_id, warehouse_id, quantity, price_cents) in lines {
        if quantity <= 0.0 {
            return Err(AppError::BadRequest("Quantity must be positive".into()));
        }

        let sale_line = sqlx::query_as::<_, SaleLine>(
            "INSERT INTO sale_lines (sale_id, product_id, quantity, price_per_unit)
             VALUES (?, ?, ?, ?) RETURNING *",
        )
        .bind(sale.id)
        .bind(product_id)
        .bind(quantity)
        .bind(price_cents)
        .fetch_one(&mut *tx)
        .await?;

        let utxos = sqlx::query_as::<_, InventoryUtxo>(
            "SELECT * FROM inventory_utxos
             WHERE product_id = ? AND warehouse_id = ? AND spent = 0
             ORDER BY created_at ASC",
        )
        .bind(product_id)
        .bind(warehouse_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut remaining = quantity;

        for utxo in &utxos {
            if remaining <= 0.0 {
                break;
            }

            let used = remaining.min(utxo.quantity);
            remaining -= used;

            sqlx::query(
                "UPDATE inventory_utxos SET spent = 1, spent_by_sale_id = ? WHERE id = ?",
            )
            .bind(sale.id)
            .bind(utxo.id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO sale_line_utxo_inputs (sale_line_id, utxo_id, quantity_used)
                 VALUES (?, ?, ?)",
            )
            .bind(sale_line.id)
            .bind(utxo.id)
            .bind(used)
            .execute(&mut *tx)
            .await?;

            if used < utxo.quantity {
                let change = utxo.quantity - used;
                sqlx::query(
                    "INSERT INTO inventory_utxos
                     (product_id, warehouse_id, quantity, cost_per_unit, source_sale_id, spent)
                     VALUES (?, ?, ?, ?, ?, 0)",
                )
                .bind(product_id)
                .bind(warehouse_id)
                .bind(change)
                .bind(utxo.cost_per_unit)
                .bind(sale.id)
                .execute(&mut *tx)
                .await?;
            }
        }

        if remaining > 0.0 {
            let available = quantity - remaining;
            return Err(AppError::InsufficientStock {
                product_id,
                requested: quantity,
                available,
            });
        }
    }

    tx.commit().await?;
    Ok(sale)
}

pub async fn create_sale(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateSaleRequest>,
) -> Result<Json<Sale>, AppError> {
    let lines: Vec<(i64, i64, f64, i64)> = body
        .lines
        .iter()
        .map(|l| (l.product_id, l.warehouse_id, l.quantity, l.price_per_unit.cents()))
        .collect();

    let sale = create_sale_tx(&pool, body.customer_id, body.customer_group_id, body.notes.as_deref(), &lines).await?;
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
