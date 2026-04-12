use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Product {
    pub id: i64,
    pub sku: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub unit: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProduct {
    pub sku: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Warehouse {
    pub id: i64,
    pub name: String,
    pub address: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWarehouse {
    pub name: String,
    pub address: Option<String>,
}
