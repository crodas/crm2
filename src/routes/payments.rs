use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use ledger::AccountPath;

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

    let debtor = AccountPath::new(&format!("@customer/{customer_id}/debt"))
        .map_err(|e| AppError::Internal(format!("invalid debtor path: {e}")))?;
    let creditor = AccountPath::new(&format!("@store/receivables/{customer_id}"))
        .map_err(|e| AppError::Internal(format!("invalid creditor path: {e}")))?;
    let gs = state
        .ledger
        .asset("gs")
        .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;

    let builder = state
        .ledger
        .transaction(format!("customer-payment-{}", payment.id));
    let builder = state
        .ledger
        .settle_debt(builder, &debtor, &creditor, &gs, amount)
        .await
        .map_err(|e| AppError::Internal(format!("settle debt: {e}")))?;
    let amount_str = format!("{amount}");
    let ledger_tx = builder
        .credit("@store/cash", "gs", &amount_str)
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
    // Total owed from quotes (SQL — quotes are not managed by ledger)
    let total_owed: Amount =
        sqlx::query_scalar("SELECT COALESCE(SUM(total_amount), 0) FROM quotes WHERE customer_id = ? AND status IN ('accepted', 'booked')")
            .bind(customer_id)
            .fetch_one(&state.pool)
            .await?;

    // Total paid from payment_utxos metadata (still in SQL)
    let total_paid: Amount =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM payment_utxos WHERE quote_id IN (SELECT id FROM quotes WHERE customer_id = ?)")
            .bind(customer_id)
            .fetch_one(&state.pool)
            .await?;

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

    let row = sqlx::query_as::<_, Row>(
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

    Ok(Json(ReceivablesBalance {
        total_owed: row.total_owed,
        total_paid: row.total_paid,
        outstanding: row.total_owed - row.total_paid,
    }))
}
