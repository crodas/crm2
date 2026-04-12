use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::quote::*;

#[derive(Deserialize)]
pub struct QuoteListParams {
    pub customer_id: Option<i64>,
    pub status: Option<String>,
}

pub async fn list_quotes(
    State(pool): State<SqlitePool>,
    Query(params): Query<QuoteListParams>,
) -> Result<Json<Vec<Quote>>, AppError> {
    let quotes = if let Some(cid) = params.customer_id {
        if let Some(status) = &params.status {
            sqlx::query_as::<_, Quote>(
                "SELECT * FROM quotes WHERE customer_id = ? AND status = ? ORDER BY created_at DESC",
            )
            .bind(cid)
            .bind(status)
            .fetch_all(&pool)
            .await?
        } else {
            sqlx::query_as::<_, Quote>(
                "SELECT * FROM quotes WHERE customer_id = ? ORDER BY created_at DESC",
            )
            .bind(cid)
            .fetch_all(&pool)
            .await?
        }
    } else if let Some(status) = &params.status {
        sqlx::query_as::<_, Quote>(
            "SELECT * FROM quotes WHERE status = ? ORDER BY created_at DESC",
        )
        .bind(status)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query_as::<_, Quote>("SELECT * FROM quotes ORDER BY created_at DESC")
            .fetch_all(&pool)
            .await?
    };
    Ok(Json(quotes))
}

pub async fn create_quote(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateQuote>,
) -> Result<Json<Quote>, AppError> {
    let mut tx = pool.begin().await?;

    let total: f64 = body.lines.iter().map(|l| l.quantity * l.unit_price).sum();

    let quote = sqlx::query_as::<_, Quote>(
        "INSERT INTO quotes (customer_id, title, description, total_amount, valid_until)
         VALUES (?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(body.customer_id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(total)
    .bind(&body.valid_until)
    .fetch_one(&mut *tx)
    .await?;

    for line in &body.lines {
        let line_type = line.line_type.as_deref().unwrap_or("item");
        sqlx::query(
            "INSERT INTO quote_lines (quote_id, description, quantity, unit_price, service_id, line_type)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(quote.id)
        .bind(&line.description)
        .bind(line.quantity)
        .bind(line.unit_price)
        .bind(line.service_id)
        .bind(line_type)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(Json(quote))
}

pub async fn get_quote(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<QuoteDetail>, AppError> {
    let quote = sqlx::query_as::<_, Quote>("SELECT * FROM quotes WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Quote not found".into()))?;

    let lines = sqlx::query_as::<_, QuoteLine>("SELECT * FROM quote_lines WHERE quote_id = ?")
        .bind(id)
        .fetch_all(&pool)
        .await?;

    let payments =
        sqlx::query_as::<_, PaymentUtxo>("SELECT * FROM payment_utxos WHERE quote_id = ?")
            .bind(id)
            .fetch_all(&pool)
            .await?;

    let total_paid: f64 = payments.iter().map(|p| p.amount).sum();
    let balance = quote.total_amount - total_paid;

    Ok(Json(QuoteDetail {
        quote,
        lines,
        payments,
        total_paid,
        balance,
    }))
}

pub async fn update_quote(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Quote>, AppError> {
    let existing = sqlx::query_as::<_, Quote>("SELECT * FROM quotes WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Quote not found".into()))?;

    let quote = sqlx::query_as::<_, Quote>(
        "UPDATE quotes SET title = ?, description = ?, valid_until = ?, updated_at = datetime('now')
         WHERE id = ? RETURNING *",
    )
    .bind(body["title"].as_str().unwrap_or(&existing.title))
    .bind(
        body["description"]
            .as_str()
            .or(existing.description.as_deref()),
    )
    .bind(
        body["valid_until"]
            .as_str()
            .or(existing.valid_until.as_deref()),
    )
    .bind(id)
    .fetch_one(&pool)
    .await?;
    Ok(Json(quote))
}

pub async fn update_quote_status(
    State(pool): State<SqlitePool>,
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

    let quote = sqlx::query_as::<_, Quote>(
        "UPDATE quotes SET status = ?, updated_at = datetime('now') WHERE id = ? RETURNING *",
    )
    .bind(&body.status)
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Quote not found".into()))?;
    Ok(Json(quote))
}

pub async fn create_debt(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateDebt>,
) -> Result<Json<Quote>, AppError> {
    let mut tx = pool.begin().await?;

    let quote = sqlx::query_as::<_, Quote>(
        "INSERT INTO quotes (customer_id, status, title, description, total_amount, is_debt)
         VALUES (?, 'accepted', ?, ?, ?, 1) RETURNING *",
    )
    .bind(body.customer_id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(body.amount)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO quote_lines (quote_id, description, quantity, unit_price)
         VALUES (?, ?, 1, ?)",
    )
    .bind(quote.id)
    .bind(&body.title)
    .bind(body.amount)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Json(quote))
}
