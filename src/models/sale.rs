use serde::{Deserialize, Serialize};

use crate::amount::Amount;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Sale {
    pub id: i64,
    pub customer_id: i64,
    pub customer_group_id: i64,
    pub notes: Option<String>,
    pub total_amount: Amount,
    pub payment_status: String,
    pub sold_at: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SaleLine {
    pub id: i64,
    pub sale_id: i64,
    pub product_id: i64,
    pub quantity: f64,
    pub price_per_unit: Amount,
    pub created_at: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SalePayment {
    pub id: i64,
    pub sale_id: i64,
    pub amount: Amount,
    pub method: Option<String>,
    pub notes: Option<String>,
    pub paid_at: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSaleRequest {
    pub customer_id: i64,
    pub customer_group_id: i64,
    pub notes: Option<String>,
    pub lines: Vec<CreateSaleLine>,
    /// If provided, the sale is paid immediately with this method.
    /// If absent/null, the sale is deferred (credit).
    pub payment_method: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSaleLine {
    pub product_id: i64,
    pub warehouse_id: i64,
    pub quantity: f64,
    pub price_per_unit: Amount,
}

#[derive(Debug, Deserialize)]
pub struct CreateSalePayment {
    pub amount: Amount,
    pub method: Option<String>,
    pub notes: Option<String>,
}
