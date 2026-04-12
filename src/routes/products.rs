use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::product::*;

pub async fn list_products(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<Product>>, AppError> {
    let products = sqlx::query_as::<_, Product>("SELECT * FROM products ORDER BY name")
        .fetch_all(&pool)
        .await?;
    Ok(Json(products))
}

pub async fn create_product(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateProduct>,
) -> Result<Json<Product>, AppError> {
    let product = sqlx::query_as::<_, Product>(
        "INSERT INTO products (sku, name, description, unit) VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(&body.sku)
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.unit.as_deref().unwrap_or("unit"))
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
        "UPDATE products SET sku = ?, name = ?, description = ?, unit = ?, updated_at = datetime('now')
         WHERE id = ? RETURNING *",
    )
    .bind(body["sku"].as_str().or(existing.sku.as_deref()))
    .bind(body["name"].as_str().unwrap_or(&existing.name))
    .bind(body["description"].as_str().or(existing.description.as_deref()))
    .bind(body["unit"].as_str().unwrap_or(&existing.unit))
    .bind(id)
    .fetch_one(&pool)
    .await?;
    Ok(Json(product))
}
