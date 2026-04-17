use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;
use crate::models::booking::*;
use crate::models::quote::Quote;
use crate::version;

pub async fn list_bookings(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Booking>>, AppError> {
    let bookings = sqlx::query_as::<_, Booking>("SELECT * FROM bookings ORDER BY start_at DESC")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(bookings))
}

pub async fn create_booking(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBooking>,
) -> Result<Json<Booking>, AppError> {
    let mut tx = state.pool.begin().await?;

    let prev_booking = version::latest_version_id(&mut *tx, "bookings").await?;
    let booking_vid = version::compute_version_id(
        &version::booking_fields(
            body.team_id,
            body.customer_id,
            &body.title,
            &body.start_at,
            &body.end_at,
            &body.notes,
            &body.description,
            &body.location,
        ),
        &prev_booking,
    );

    let booking = sqlx::query_as::<_, Booking>(
        "INSERT INTO bookings (team_id, customer_id, title, start_at, end_at, notes, description, location, version_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(body.team_id)
    .bind(body.customer_id)
    .bind(&body.title)
    .bind(&body.start_at)
    .bind(&body.end_at)
    .bind(&body.notes)
    .bind(&body.description)
    .bind(&body.location)
    .bind(&booking_vid)
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
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let booking = sqlx::query_as::<_, Booking>("SELECT * FROM bookings WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found".into()))?;

    let quotes = sqlx::query_as::<_, Quote>(
        "SELECT q.* FROM quotes q
         JOIN booking_quotes bq ON bq.quote_id = q.id
         WHERE bq.booking_id = ?",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "booking": booking,
        "quotes": quotes,
    })))
}

pub async fn update_booking(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Booking>, AppError> {
    let existing = sqlx::query_as::<_, Booking>("SELECT * FROM bookings WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found".into()))?;

    let team_id = body["team_id"].as_i64().unwrap_or(existing.team_id);
    let booking = sqlx::query_as::<_, Booking>(
        "UPDATE bookings SET
            title = ?, start_at = ?, end_at = ?, status = ?, notes = ?, description = ?, location = ?, team_id = ?, updated_at = datetime('now')
         WHERE id = ? RETURNING *",
    )
    .bind(body["title"].as_str().unwrap_or(&existing.title))
    .bind(body["start_at"].as_str().unwrap_or(&existing.start_at))
    .bind(body["end_at"].as_str().unwrap_or(&existing.end_at))
    .bind(body["status"].as_str().unwrap_or(&existing.status))
    .bind(body["notes"].as_str().or(existing.notes.as_deref()))
    .bind(body["description"].as_str().or(existing.description.as_deref()))
    .bind(body["location"].as_str().or(existing.location.as_deref()))
    .bind(team_id)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(booking))
}

pub async fn link_quote(
    State(state): State<Arc<AppState>>,
    Path((booking_id, quote_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("INSERT OR IGNORE INTO booking_quotes (booking_id, quote_id) VALUES (?, ?)")
        .bind(booking_id)
        .bind(quote_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn unlink_quote(
    State(state): State<Arc<AppState>>,
    Path((booking_id, quote_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("DELETE FROM booking_quotes WHERE booking_id = ? AND quote_id = ?")
        .bind(booking_id)
        .bind(quote_id)
        .execute(&state.pool)
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}
