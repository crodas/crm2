use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::amount::Amount;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Product {
    pub id: i64,
    pub sku: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub unit: String,
    pub product_type: String,
    pub suggested_price: Amount,
    pub created_at: String,
    pub updated_at: String,
}

/// Product with latest prices per customer group name
#[derive(Debug, Serialize)]
pub struct ProductWithPrices {
    #[serde(flatten)]
    pub product: Product,
    /// Map of customer group name → latest price
    pub prices: HashMap<String, Amount>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProduct {
    pub sku: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub unit: Option<String>,
    pub product_type: Option<String>,
    pub suggested_price: Option<Amount>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Warehouse {
    pub id: i64,
    pub name: String,
    pub address: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWarehouse {
    pub name: String,
    pub address: Option<String>,
}
