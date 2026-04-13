use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::booking::*;
use crate::models::quote::Quote;

pub async fn list_bookings(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<Booking>>, AppError> {
    let bookings =
        sqlx::query_as::<_, Booking>("SELECT * FROM bookings ORDER BY start_at DESC")
            .fetch_all(&pool)
            .await?;
    Ok(Json(bookings))
}

pub async fn create_booking(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateBooking>,
) -> Result<Json<Booking>, AppError> {
    let mut tx = pool.begin().await?;

    let booking = sqlx::query_as::<_, Booking>(
        "INSERT INTO bookings (team_id, customer_id, title, start_at, end_at, notes)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(body.team_id)
    .bind(body.customer_id)
    .bind(&body.title)
    .bind(&body.start_at)
    .bind(&body.end_at)
    .bind(&body.notes)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(quote_ids) = &body.quote_ids {
        for qid in quote_ids {
            sqlx::query("INSERT INTO booking_quotes (booking_id, quote_id) VALUES (?, ?)")
                .bind(booking.id)
                .bind(qid)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(Json(booking))
}

pub async fn get_booking(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let booking = sqlx::query_as::<_, Booking>("SELECT * FROM bookings WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found".into()))?;

    let work_orders = sqlx::query_as::<_, WorkOrder>(
        "SELECT * FROM work_orders WHERE booking_id = ?",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;

    let quotes = sqlx::query_as::<_, Quote>(
        "SELECT q.* FROM quotes q
         JOIN booking_quotes bq ON bq.quote_id = q.id
         WHERE bq.booking_id = ?",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(serde_json::json!({
        "booking": booking,
        "work_orders": work_orders,
        "quotes": quotes,
    })))
}

pub async fn update_booking(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Booking>, AppError> {
    let existing = sqlx::query_as::<_, Booking>("SELECT * FROM bookings WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found".into()))?;

    let team_id = body["team_id"].as_i64().unwrap_or(existing.team_id);
    let booking = sqlx::query_as::<_, Booking>(
        "UPDATE bookings SET
            title = ?, start_at = ?, end_at = ?, status = ?, notes = ?, team_id = ?, updated_at = datetime('now')
         WHERE id = ? RETURNING *",
    )
    .bind(body["title"].as_str().unwrap_or(&existing.title))
    .bind(body["start_at"].as_str().unwrap_or(&existing.start_at))
    .bind(body["end_at"].as_str().unwrap_or(&existing.end_at))
    .bind(body["status"].as_str().unwrap_or(&existing.status))
    .bind(body["notes"].as_str().or(existing.notes.as_deref()))
    .bind(team_id)
    .bind(id)
    .fetch_one(&pool)
    .await?;
    Ok(Json(booking))
}

pub async fn link_quote(
    State(pool): State<SqlitePool>,
    Path((booking_id, quote_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("INSERT OR IGNORE INTO booking_quotes (booking_id, quote_id) VALUES (?, ?)")
        .bind(booking_id)
        .bind(quote_id)
        .execute(&pool)
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn unlink_quote(
    State(pool): State<SqlitePool>,
    Path((booking_id, quote_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("DELETE FROM booking_quotes WHERE booking_id = ? AND quote_id = ?")
        .bind(booking_id)
        .bind(quote_id)
        .execute(&pool)
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn create_work_order(
    State(pool): State<SqlitePool>,
    Path(booking_id): Path<i64>,
    Json(body): Json<CreateWorkOrder>,
) -> Result<Json<WorkOrder>, AppError> {
    let booking = sqlx::query_as::<_, Booking>("SELECT * FROM bookings WHERE id = ?")
        .bind(booking_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found".into()))?;

    let wo = sqlx::query_as::<_, WorkOrder>(
        "INSERT INTO work_orders (booking_id, customer_id, description, location)
         VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(booking_id)
    .bind(booking.customer_id)
    .bind(&body.description)
    .bind(&body.location)
    .fetch_one(&pool)
    .await?;
    Ok(Json(wo))
}
