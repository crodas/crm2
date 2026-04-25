use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::product::*;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ProductQuery {
    pub product_type: Option<String>,
}

pub async fn list_products(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ProductQuery>,
) -> Result<Json<Vec<ProductWithPrices>>, AppError> {
    let products = state.db.list_products(params.product_type.as_deref()).await?;
    let prices_map = state.db.fetch_latest_prices().await?;
    let result: Vec<ProductWithPrices> = products
        .into_iter()
        .map(|p| {
            let prices = prices_map.get(&p.id).cloned().unwrap_or_default();
            ProductWithPrices { product: p, prices }
        })
        .collect();
    Ok(Json(result))
}

pub async fn create_product(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProduct>,
) -> Result<Json<ProductWithPrices>, AppError> {
    let mut tx = state.db.begin().await?;
    let product = tx.create_product(&body).await?;
    tx.commit().await?;
    Ok(Json(ProductWithPrices {
        product,
        prices: Default::default(),
    }))
}

pub async fn get_product(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<ProductWithPrices>, AppError> {
    let product = state.db.get_product(id).await?;
    let prices_map = state.db.fetch_latest_prices().await?;
    let prices = prices_map.get(&product.id).cloned().unwrap_or_default();
    Ok(Json(ProductWithPrices { product, prices }))
}

pub async fn update_product(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ProductWithPrices>, AppError> {
    let existing = state.db.get_product(id).await?;
    let mut tx = state.db.begin().await?;
    let product = tx.update_product(id, &body, &existing).await?;
    tx.commit().await?;
    let prices_map = state.db.fetch_latest_prices().await?;
    let prices = prices_map.get(&product.id).cloned().unwrap_or_default();
    Ok(Json(ProductWithPrices { product, prices }))
}
