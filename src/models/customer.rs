use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CustomerType {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Customer {
    pub id: i64,
    pub customer_type_id: i64,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCustomer {
    pub customer_type_id: i64,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCustomer {
    pub customer_type_id: Option<i64>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CustomerGroup {
    pub id: i64,
    pub name: String,
    pub customer_type_id: i64,
    pub default_markup_pct: f64,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCustomerGroup {
    pub name: String,
    pub customer_type_id: i64,
    pub default_markup_pct: f64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TimelineEvent {
    pub event_type: String,
    pub id: i64,
    pub summary: String,
    pub date: String,
    pub amount: Option<f64>,
}
