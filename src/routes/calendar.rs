use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;
use crate::models::booking::Booking;

#[derive(Deserialize)]
pub struct CalendarQuery {
    pub team_id: Option<i64>,
    pub start: String,
    pub end: String,
}

pub async fn get_calendar(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CalendarQuery>,
) -> Result<Json<Vec<Booking>>, AppError> {
    let bookings = if let Some(team_id) = params.team_id {
        sqlx::query_as::<_, Booking>(
            "SELECT * FROM bookings
             WHERE team_id = ? AND start_at >= ? AND start_at <= ?
             ORDER BY start_at ASC",
        )
        .bind(team_id)
        .bind(&params.start)
        .bind(&params.end)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, Booking>(
            "SELECT * FROM bookings
             WHERE start_at >= ? AND start_at <= ?
             ORDER BY start_at ASC",
        )
        .bind(&params.start)
        .bind(&params.end)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(bookings))
}
