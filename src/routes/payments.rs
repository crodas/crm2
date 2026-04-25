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
    let quote = state.db.get_quote(quote_id).await?;

    if body.amount.0 <= 0 {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    let mut tx = state.db.begin().await?;
    let payment = tx.record_quote_payment(quote_id, &body).await?;
    tx.settle_customer_debt(
        quote.customer_id,
        body.amount.cents(),
        &format!("customer-payment-{}", payment.id),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(payment))
}

pub async fn customer_balance(
    State(state): State<Arc<AppState>>,
    Path(customer_id): Path<i64>,
) -> Result<Json<CustomerBalance>, AppError> {
    let quote_owed = state.db.customer_quote_owed(customer_id).await?;
    let sale_owed = state.db.customer_sale_owed(customer_id).await?;
    let total_owed = quote_owed + sale_owed;

    let quote_paid = state.db.customer_quote_paid(customer_id).await?;
    let sale_paid = state.db.customer_sale_paid(customer_id).await?;
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
    let (quote_owed, quote_paid) = state.db.total_quote_receivables().await?;
    let (sale_owed, sale_paid) = state.db.total_sale_receivables().await?;

    let total_owed = quote_owed + sale_owed;
    let total_paid = quote_paid + sale_paid;

    Ok(Json(ReceivablesBalance {
        total_owed,
        total_paid,
        outstanding: total_owed - total_paid,
    }))
}
