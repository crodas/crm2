use serde::{Deserialize, Serialize};

use crate::amount::Amount;
use crate::models::booking::Booking;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Quote {
    pub id: i64,
    pub customer_id: i64,
    pub status: String,
    pub title: String,
    pub description: Option<String>,
    pub total_amount: Amount,
    pub is_debt: bool,
    pub valid_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version_id: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct QuoteLine {
    pub id: i64,
    pub quote_id: i64,
    pub description: String,
    pub quantity: f64,
    pub unit_price: Amount,
    pub service_id: Option<i64>,
    pub line_type: String,
    pub created_at: String,
    pub version_id: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PaymentUtxo {
    pub id: i64,
    pub quote_id: i64,
    pub amount: Amount,
    pub method: Option<String>,
    pub notes: Option<String>,
    pub paid_at: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateQuote {
    pub customer_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub valid_until: Option<String>,
    pub lines: Vec<CreateQuoteLine>,
}

#[derive(Debug, Deserialize)]
pub struct CreateQuoteLine {
    pub description: String,
    pub quantity: f64,
    pub unit_price: Amount,
    pub service_id: Option<i64>,
    pub line_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateQuoteStatus {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatePayment {
    pub amount: Amount,
    pub method: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDebt {
    pub customer_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub amount: Amount,
}

#[derive(Debug, Serialize)]
pub struct QuoteDetail {
    pub quote: Quote,
    pub lines: Vec<QuoteLine>,
    pub payments: Vec<PaymentUtxo>,
    pub total_paid: Amount,
    pub balance: Amount,
    pub bookings: Vec<Booking>,
}
