use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::product::*;

#[derive(Deserialize)]
pub struct ProductQuery {
    pub product_type: Option<String>,
}

pub async fn list_products(
    State(pool): State<SqlitePool>,
    Query(params): Query<ProductQuery>,
) -> Result<Json<Vec<Product>>, AppError> {
    let products = if let Some(pt) = &params.product_type {
        sqlx::query_as::<_, Product>(
            "SELECT * FROM products WHERE product_type = ? ORDER BY name",
        )
        .bind(pt)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query_as::<_, Product>("SELECT * FROM products ORDER BY product_type, name")
            .fetch_all(&pool)
            .await?
    };
    Ok(Json(products))
}

pub async fn create_product(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateProduct>,
) -> Result<Json<Product>, AppError> {
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
    .bind(body.suggested_price.unwrap_or(0.0))
    .fetch_one(&pool)
    .await?;
    Ok(Json(product))
}

pub async fn get_product(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<Product>, AppError> {
    let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
    Ok(Json(product))
}

pub async fn update_product(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Product>, AppError> {
    let existing = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
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
    .bind(body["suggested_price"].as_f64().unwrap_or(existing.suggested_price))
    .bind(id)
    .fetch_one(&pool)
    .await?;
    Ok(Json(product))
}
