use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

use ledger::Asset;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::product::*;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ProductQuery {
    pub product_type: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PriceRow {
    product_id: i64,
    group_name: String,
    price_per_unit: Amount,
}

async fn fetch_latest_prices(
    pool: &SqlitePool,
) -> Result<HashMap<i64, HashMap<String, Amount>>, sqlx::Error> {
    let rows = sqlx::query_as::<_, PriceRow>(
        "SELECT p.product_id, cg.name as group_name, p.price_per_unit
         FROM inventory_receipt_prices p
         INNER JOIN customer_groups cg ON cg.id = p.customer_group_id
         INNER JOIN (
             SELECT product_id, customer_group_id, MAX(receipt_id) as max_receipt_id
             FROM inventory_receipt_prices
             GROUP BY product_id, customer_group_id
         ) latest ON p.product_id = latest.product_id
                  AND p.customer_group_id = latest.customer_group_id
                  AND p.receipt_id = latest.max_receipt_id",
    )
    .fetch_all(pool)
    .await?;

    let mut map: HashMap<i64, HashMap<String, Amount>> = HashMap::new();
    for row in rows {
        map.entry(row.product_id)
            .or_default()
            .insert(row.group_name, row.price_per_unit);
    }
    Ok(map)
}

fn enrich(
    product: Product,
    prices_map: &HashMap<i64, HashMap<String, Amount>>,
) -> ProductWithPrices {
    let prices = prices_map.get(&product.id).cloned().unwrap_or_default();
    ProductWithPrices { product, prices }
}

pub async fn list_products(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ProductQuery>,
) -> Result<Json<Vec<ProductWithPrices>>, AppError> {
    let products = if let Some(pt) = &params.product_type {
        sqlx::query_as::<_, Product>("SELECT * FROM products WHERE product_type = ? ORDER BY name")
            .bind(pt)
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Product>("SELECT * FROM products ORDER BY product_type, name")
            .fetch_all(&state.pool)
            .await?
    };

    let prices_map = fetch_latest_prices(&state.pool).await?;
    let result: Vec<ProductWithPrices> = products
        .into_iter()
        .map(|p| enrich(p, &prices_map))
        .collect();
    Ok(Json(result))
}

pub async fn create_product(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProduct>,
) -> Result<Json<ProductWithPrices>, AppError> {
    let pt = body.product_type.as_deref().unwrap_or("product");
    let sku = match &body.sku {
        Some(s) if !s.is_empty() => s.clone(),
        _ if pt == "service" => format!("SVC-{}", uuid::Uuid::new_v4()),
        _ => String::new(),
    };
    let sku_opt = if sku.is_empty() { None } else { Some(sku) };

    let product = sqlx::query_as::<_, Product>(
        "INSERT INTO products (sku, name, description, unit, product_type, suggested_price)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(&sku_opt)
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.unit.as_deref().unwrap_or("unit"))
    .bind(pt)
    .bind(body.suggested_price.unwrap_or(Amount(0)))
    .fetch_one(&state.pool)
    .await?;

    // Register a ledger asset for this product
    state
        .ledger
        .register_asset(Asset::new(
            format!("product:{}", product.id),
            3,
        ))
        .await
        .map_err(|e| AppError::Internal(format!("register asset: {e}")))?;

    Ok(Json(ProductWithPrices {
        product,
        prices: HashMap::new(),
    }))
}

pub async fn get_product(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<ProductWithPrices>, AppError> {
    let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

    let prices_map = fetch_latest_prices(&state.pool).await?;
    Ok(Json(enrich(product, &prices_map)))
}

pub async fn update_product(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ProductWithPrices>, AppError> {
    let existing = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

    let product = sqlx::query_as::<_, Product>(
        "UPDATE products SET sku = ?, name = ?, description = ?, unit = ?, product_type = ?, suggested_price = ?, updated_at = datetime('now')
         WHERE id = ? RETURNING *",
    )
    .bind(body["sku"].as_str().or(existing.sku.as_deref()))
    .bind(body["name"].as_str().unwrap_or(&existing.name))
    .bind(body["description"].as_str().or(existing.description.as_deref()))
    .bind(body["unit"].as_str().unwrap_or(&existing.unit))
    .bind(body["product_type"].as_str().unwrap_or(&existing.product_type))
    .bind(body["suggested_price"].as_f64().map(Amount::from_float).unwrap_or(existing.suggested_price))
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let prices_map = fetch_latest_prices(&state.pool).await?;
    Ok(Json(enrich(product, &prices_map)))
}
