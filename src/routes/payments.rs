use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::quote::*;

#[derive(Serialize)]
pub struct CustomerBalance {
    pub customer_id: i64,
    pub total_owed: Amount,
    pub total_paid: Amount,
    pub outstanding: Amount,
}

pub async fn record_payment(
    State(pool): State<SqlitePool>,
    Path(quote_id): Path<i64>,
    Json(body): Json<CreatePayment>,
) -> Result<Json<PaymentUtxo>, AppError> {
    let _quote = sqlx::query_as::<_, Quote>("SELECT * FROM quotes WHERE id = ?")
        .bind(quote_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Quote not found".into()))?;

    if body.amount.0 <= 0 {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    let payment = sqlx::query_as::<_, PaymentUtxo>(
        "INSERT INTO payment_utxos (quote_id, amount, method, notes) VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(quote_id)
    .bind(body.amount)
    .bind(&body.method)
    .bind(&body.notes)
    .fetch_one(&pool)
    .await?;
    Ok(Json(payment))
}

pub async fn customer_balance(
    State(pool): State<SqlitePool>,
    Path(customer_id): Path<i64>,
) -> Result<Json<CustomerBalance>, AppError> {
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
         WHERE q.customer_id = ? AND q.status IN ('accepted', 'booked')",
    )
    .bind(customer_id)
    .fetch_one(&pool)
    .await?;

    Ok(Json(CustomerBalance {
        customer_id,
        total_owed: row.total_owed,
        total_paid: row.total_paid,
        outstanding: row.total_owed - row.total_paid,
    }))
}
