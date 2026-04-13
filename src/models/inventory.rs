use serde::{Deserialize, Serialize};

use crate::amount::Amount;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InventoryReceipt {
    pub id: i64,
    pub reference: Option<String>,
    pub supplier_name: Option<String>,
    pub notes: Option<String>,
    pub received_at: String,
    pub created_at: String,
    pub version_id: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InventoryUtxo {
    pub id: i64,
    pub product_id: i64,
    pub warehouse_id: i64,
    pub quantity: f64,
    pub cost_per_unit: Amount,
    pub receipt_id: Option<i64>,
    pub source_sale_id: Option<i64>,
    pub spent: bool,
    pub spent_by_sale_id: Option<i64>,
    pub created_at: String,
    pub version_id: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StockLevel {
    pub product_id: i64,
    pub warehouse_id: i64,
    pub total_quantity: f64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReceiptPrice {
    pub id: i64,
    pub receipt_id: i64,
    pub product_id: i64,
    pub customer_group_id: i64,
    pub price_per_unit: Amount,
    pub version_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ReceiveInventoryRequest {
    pub reference: Option<String>,
    pub supplier_name: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<ReceiveInventoryLine>,
}

#[derive(Debug, Deserialize)]
pub struct ReceiveInventoryLine {
    pub product_id: i64,
    pub warehouse_id: i64,
    pub quantity: f64,
    pub cost_per_unit: Amount,
    pub prices: Vec<LinePrice>,
}

#[derive(Debug, Deserialize)]
pub struct LinePrice {
    pub customer_group_id: i64,
    pub price_per_unit: Amount,
}

#[derive(Debug, Deserialize)]
pub struct LatestPriceQuery {
    pub product_id: Option<i64>,
    pub customer_group_id: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LatestPrice {
    pub product_id: i64,
    pub customer_group_id: i64,
    pub price_per_unit: Amount,
}
