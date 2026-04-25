use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::booking::Booking;
use crate::state::AppState;

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
    let bookings = state
        .db
        .calendar(params.team_id, &params.start, &params.end)
        .await?;
    Ok(Json(bookings))
}
