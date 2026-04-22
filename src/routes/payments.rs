use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::quote::*;
use crate::state::AppState;

#[derive(Serialize)]
pub struct CustomerBalance {
    pub customer_id: i64,
    pub total_owed: Amount,
    pub total_paid: Amount,
    pub outstanding: Amount,
}

pub async fn record_payment(
    State(state): State<Arc<AppState>>,
    Path(quote_id): Path<i64>,
    Json(body): Json<CreatePayment>,
) -> Result<Json<PaymentUtxo>, AppError> {
    let quote = sqlx::query_as::<_, Quote>("SELECT * FROM quotes WHERE id = ?")
        .bind(quote_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Quote not found".into()))?;

    if body.amount.0 <= 0 {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    // Record metadata in payment_utxos table
    let payment = sqlx::query_as::<_, PaymentUtxo>(
        "INSERT INTO payment_utxos (quote_id, amount, method, notes) VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(quote_id)
    .bind(body.amount)
    .bind(&body.method)
    .bind(&body.notes)
    .fetch_one(&state.pool)
    .await?;

    // Record in ledger: settle debt + cash leg
    let amount: i128 = body.amount.cents().into();
    let customer_id = quote.customer_id;

    let gs = state
        .ledger
        .asset("gs")
        .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;

    let gs_amount = gs
        .try_amount(amount)
        .map_err(|e| AppError::Internal(format!("gs amount: {e}")))?;
    let ledger_tx = state
        .ledger
        .transaction(format!("customer-payment-{}", payment.id))
        .settle_debt(&customer_id.to_string(), &gs_amount)
        .await
        .map_err(|e| AppError::Internal(format!("settle debt: {e}")))?
        .credit("store/cash", &gs_amount)
        .build()
        .await
        .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
    state
        .ledger
        .commit(ledger_tx)
        .await
        .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

    Ok(Json(payment))
}

pub async fn customer_balance(
    State(state): State<Arc<AppState>>,
    Path(customer_id): Path<i64>,
) -> Result<Json<CustomerBalance>, AppError> {
    // Total owed from quotes + sales
    let quote_owed: Amount =
        sqlx::query_scalar("SELECT COALESCE(SUM(total_amount), 0) FROM quotes WHERE customer_id = ? AND status IN ('accepted', 'booked')")
            .bind(customer_id)
            .fetch_one(&state.pool)
            .await?;
    let sale_owed: Amount = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_amount), 0) FROM sales WHERE customer_id = ?",
    )
    .bind(customer_id)
    .fetch_one(&state.pool)
    .await?;
    let total_owed = quote_owed + sale_owed;

    // Total paid from payment_utxos (quotes) + sale_payments
    let quote_paid: Amount =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM payment_utxos WHERE quote_id IN (SELECT id FROM quotes WHERE customer_id = ?)")
            .bind(customer_id)
            .fetch_one(&state.pool)
            .await?;
    let sale_paid: Amount =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM sale_payments WHERE sale_id IN (SELECT id FROM sales WHERE customer_id = ?)")
            .bind(customer_id)
            .fetch_one(&state.pool)
            .await?;
    let total_paid = quote_paid + sale_paid;

    Ok(Json(CustomerBalance {
        customer_id,
        total_owed,
        total_paid,
        outstanding: total_owed - total_paid,
    }))
}

#[derive(Serialize)]
pub struct ReceivablesBalance {
    pub total_owed: Amount,
    pub total_paid: Amount,
    pub outstanding: Amount,
}

pub async fn total_receivables(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReceivablesBalance>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        total_owed: Amount,
        total_paid: Amount,
    }

    let quote_row = sqlx::query_as::<_, Row>(
        "SELECT
            COALESCE(SUM(q.total_amount), 0) as total_owed,
            COALESCE(SUM(COALESCE(p.paid, 0)), 0) as total_paid
         FROM quotes q
         LEFT JOIN (
            SELECT quote_id, SUM(amount) as paid
            FROM payment_utxos
            GROUP BY quote_id
         ) p ON p.quote_id = q.id
         WHERE q.status IN ('accepted', 'booked')",
    )
    .fetch_one(&state.pool)
    .await?;

    let sale_row = sqlx::query_as::<_, Row>(
        "SELECT
            COALESCE(SUM(s.total_amount), 0) as total_owed,
            COALESCE(SUM(COALESCE(p.paid, 0)), 0) as total_paid
         FROM sales s
         LEFT JOIN (
            SELECT sale_id, SUM(amount) as paid
            FROM sale_payments
            GROUP BY sale_id
         ) p ON p.sale_id = s.id",
    )
    .fetch_one(&state.pool)
    .await?;

    let total_owed = quote_row.total_owed + sale_row.total_owed;
    let total_paid = quote_row.total_paid + sale_row.total_paid;

    Ok(Json(ReceivablesBalance {
        total_owed,
        total_paid,
        outstanding: total_owed - total_paid,
    }))
}
