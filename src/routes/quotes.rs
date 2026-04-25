use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::quote::*;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct QuoteListParams {
    pub customer_id: Option<i64>,
    pub status: Option<String>,
}

pub async fn list_quotes(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QuoteListParams>,
) -> Result<Json<Vec<Quote>>, AppError> {
    let quotes = state
        .db
        .list_quotes(params.customer_id, params.status.as_deref())
        .await?;
    Ok(Json(quotes))
}

pub async fn create_quote(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateQuote>,
) -> Result<Json<Quote>, AppError> {
    let mut tx = state.db.begin().await?;
    let quote = tx.create_quote(&body).await?;
    tx.commit().await?;
    Ok(Json(quote))
}

pub async fn get_quote(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<QuoteDetail>, AppError> {
    let quote = state.db.get_quote(id).await?;
    let lines = state.db.get_quote_lines(id).await?;
    let payments = state.db.get_quote_payments(id).await?;
    let bookings = state.db.get_quote_bookings(id).await?;

    let total_paid: crate::amount::Amount = payments.iter().map(|p| p.amount).sum();
    let balance = quote.total_amount - total_paid;

    Ok(Json(QuoteDetail {
        quote,
        lines,
        payments,
        total_paid,
        balance,
        bookings,
    }))
}

pub async fn update_quote(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Quote>, AppError> {
    let existing = state.db.get_quote(id).await?;
    let mut tx = state.db.begin().await?;
    let quote = tx.update_quote(id, &body, &existing).await?;
    tx.commit().await?;
    Ok(Json(quote))
}

pub async fn update_quote_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateQuoteStatus>,
) -> Result<Json<Quote>, AppError> {
    let valid = ["draft", "sent", "follow_up", "accepted", "booked"];
    if !valid.contains(&body.status.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Invalid status. Must be one of: {}",
            valid.join(", ")
        )));
    }
    let mut tx = state.db.begin().await?;
    let quote = tx.update_quote_status(id, &body.status).await?;
    tx.commit().await?;
    Ok(Json(quote))
}

pub async fn create_debt(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateDebt>,
) -> Result<Json<Quote>, AppError> {
    let mut tx = state.db.begin().await?;
    let quote = tx.create_debt(&body).await?;
    tx.commit().await?;
    Ok(Json(quote))
}
