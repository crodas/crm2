use serde::{Deserialize, Serialize};

use crate::amount::Amount;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Sale {
    pub id: i64,
    pub customer_id: i64,
    pub customer_group_id: i64,
    pub notes: Option<String>,
    pub total_amount: Amount,
    pub sold_at: String,
    pub created_at: String,
    pub version_id: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SaleLine {
    pub id: i64,
    pub sale_id: i64,
    pub product_id: i64,
    pub quantity: f64,
    pub price_per_unit: Amount,
    pub created_at: String,
    pub version_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSaleRequest {
    pub customer_id: i64,
    pub customer_group_id: i64,
    pub notes: Option<String>,
    pub lines: Vec<CreateSaleLine>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSaleLine {
    pub product_id: i64,
    pub warehouse_id: i64,
    pub quantity: f64,
    pub price_per_unit: Amount,
}
