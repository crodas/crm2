use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Booking {
    pub id: i64,
    pub team_id: i64,
    pub customer_id: i64,
    pub title: String,
    pub start_at: String,
    pub end_at: String,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version_id: String,
    pub description: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBooking {
    pub team_id: i64,
    pub customer_id: i64,
    pub title: String,
    pub start_at: String,
    pub end_at: String,
    pub notes: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub quote_ids: Option<Vec<i64>>,
}
