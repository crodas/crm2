use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use ledger::AccountPath;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::sale::*;
use crate::state::AppState;

/// Core sale creation logic, usable from both the HTTP handler and tests.
/// Each tuple in `lines` is `(product_id, warehouse_id, quantity, price_per_unit_cents)`.
pub async fn create_sale_tx(
    state: &AppState,
    customer_id: i64,
    customer_group_id: i64,
    notes: Option<&str>,
    lines: &[(i64, i64, f64, i64)],
) -> Result<Sale, AppError> {
    let total: Amount = lines
        .iter()
        .map(|&(_, _, qty, price)| Amount(price).mul_qty(qty))
        .sum();

    // Insert sale metadata
    let sale = sqlx::query_as::<_, Sale>(
        "INSERT INTO sales (customer_id, customer_group_id, notes, total_amount)
         VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(customer_id)
    .bind(customer_group_id)
    .bind(notes)
    .bind(total)
    .fetch_one(&state.pool)
    .await?;

    // Insert sale lines
    for &(product_id, _warehouse_id, quantity, price_cents) in lines {
        if quantity <= 0.0 {
            return Err(AppError::BadRequest("Quantity must be positive".into()));
        }

        sqlx::query(
            "INSERT INTO sale_lines (sale_id, product_id, quantity, price_per_unit)
             VALUES (?, ?, ?, ?)",
        )
        .bind(sale.id)
        .bind(product_id)
        .bind(quantity)
        .bind(price_cents)
        .execute(&state.pool)
        .await?;
    }

    // Build ledger transaction: debit inventory, credit sold sink + customer debt
    let mut builder = state.ledger.transaction(format!("sale-{}", sale.id));

    for &(product_id, warehouse_id, quantity, _price_cents) in lines {
        let account = format!("@store/{warehouse_id}/product/{product_id}");
        let asset = format!("product:{product_id}");
        let qty = format!("{quantity:.3}");

        builder = builder
            .debit(&account, &asset, &qty)
            .credit("@sold", &asset, &qty);
    }

    // Customer debt via SignedPositionDebt strategy
    let debtor = AccountPath::new(&format!("@customer/{customer_id}/debt"))
        .map_err(|e| AppError::Internal(format!("invalid debtor path: {e}")))?;
    let creditor = AccountPath::new(&format!("@store/receivables/{customer_id}"))
        .map_err(|e| AppError::Internal(format!("invalid creditor path: {e}")))?;
    let gs = state
        .ledger
        .asset("gs")
        .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;
    builder = state
        .ledger
        .issue_debt(builder, &debtor, &creditor, &gs, total.cents().into())
        .map_err(|e| AppError::Internal(format!("issue debt: {e}")))?;

    let ledger_tx = builder.build().await.map_err(|e| match e {
        ledger::Error::InsufficientBalance {
            account,
            asset: _,
            required,
            available,
        } => {
            // Parse product_id from account "@store/{wh}/product/{pid}"
            let product_id = account
                .split('/')
                .last()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let asset_obj = state.ledger.asset(&format!("product:{product_id}"));
            let divisor = asset_obj
                .map(|a| 10_f64.powi(a.precision() as i32))
                .unwrap_or(1000.0);
            AppError::InsufficientStock {
                product_id,
                requested: required as f64 / divisor,
                available: available as f64 / divisor,
            }
        }
        other => AppError::Internal(format!("ledger build: {other}")),
    })?;

    state
        .ledger
        .commit(ledger_tx)
        .await
        .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

    Ok(sale)
}

pub async fn create_sale(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSaleRequest>,
) -> Result<Json<Sale>, AppError> {
    let lines: Vec<(i64, i64, f64, i64)> = body
        .lines
        .iter()
        .map(|l| {
            (
                l.product_id,
                l.warehouse_id,
                l.quantity,
                l.price_per_unit.cents(),
            )
        })
        .collect();

    let sale = create_sale_tx(
        &state,
        body.customer_id,
        body.customer_group_id,
        body.notes.as_deref(),
        &lines,
    )
    .await?;
    Ok(Json(sale))
}

pub async fn list_sales(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Sale>>, AppError> {
    let sales = sqlx::query_as::<_, Sale>("SELECT * FROM sales ORDER BY sold_at DESC")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(sales))
}

pub async fn get_sale(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sale = sqlx::query_as::<_, Sale>("SELECT * FROM sales WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Sale not found".into()))?;

    let lines = sqlx::query_as::<_, SaleLine>("SELECT * FROM sale_lines WHERE sale_id = ?")
        .bind(id)
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({
        "sale": sale,
        "lines": lines,
    })))
}
