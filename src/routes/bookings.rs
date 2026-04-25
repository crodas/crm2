use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::error::AppError;
use crate::models::booking::*;
use crate::state::AppState;

pub async fn list_bookings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Booking>>, AppError> {
    let bookings = state.db.list_bookings().await?;
    Ok(Json(bookings))
}

pub async fn create_booking(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBooking>,
) -> Result<Json<Booking>, AppError> {
    let mut tx = state.db.begin().await?;
    let booking = tx.create_booking(&body).await?;
    tx.commit().await?;
    Ok(Json(booking))
}

pub async fn get_booking(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let booking = state.db.get_booking(id).await?;
    let quotes = state.db.get_booking_quotes(id).await?;
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
    let existing = state.db.get_booking(id).await?;
    let mut tx = state.db.begin().await?;
    let booking = tx.update_booking(id, &body, &existing).await?;
    tx.commit().await?;
    Ok(Json(booking))
}

pub async fn link_quote(
    State(state): State<Arc<AppState>>,
    Path((booking_id, quote_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut tx = state.db.begin().await?;
    tx.link_quote(booking_id, quote_id).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn unlink_quote(
    State(state): State<Arc<AppState>>,
    Path((booking_id, quote_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut tx = state.db.begin().await?;
    tx.unlink_quote(booking_id, quote_id).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({"ok": true})))
}
