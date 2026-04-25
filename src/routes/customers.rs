use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::customer::*;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListParams {
    pub customer_type_id: Option<i64>,
    pub search: Option<String>,
}

pub async fn list_customer_types(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CustomerType>>, AppError> {
    let types = state.db.list_customer_types().await?;
    Ok(Json(types))
}

pub async fn reorder_customer_types(
    State(state): State<Arc<AppState>>,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<Vec<CustomerType>>, AppError> {
    let mut tx = state.db.begin().await?;
    tx.reorder_customer_types(&ids).await?;
    tx.commit().await?;
    list_customer_types(State(state.clone())).await
}

pub async fn update_customer_type(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<CustomerType>, AppError> {
    let name = body["name"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("name is required".into()))?;
    let mut tx = state.db.begin().await?;
    let ct = tx.update_customer_type(id, name).await?;
    tx.commit().await?;
    Ok(Json(ct))
}

pub async fn create_customer_type(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<CustomerType>, AppError> {
    let name = body["name"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("name is required".into()))?;
    let mut tx = state.db.begin().await?;
    let ct = tx.create_customer_type(name).await?;
    tx.commit().await?;
    Ok(Json(ct))
}

pub async fn list_customers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Customer>>, AppError> {
    let customers = state
        .db
        .list_customers(params.customer_type_id, params.search.as_deref())
        .await?;
    Ok(Json(customers))
}

pub async fn get_customer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Customer>, AppError> {
    let customer = state.db.get_customer(id).await?;
    Ok(Json(customer))
}

pub async fn create_customer(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateCustomer>,
) -> Result<Json<Customer>, AppError> {
    let mut tx = state.db.begin().await?;
    let customer = tx.create_customer(&body).await?;
    tx.commit().await?;
    Ok(Json(customer))
}

pub async fn update_customer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCustomer>,
) -> Result<Json<Customer>, AppError> {
    let existing = state.db.get_customer(id).await?;
    let mut tx = state.db.begin().await?;
    let customer = tx.update_customer(id, &body, &existing).await?;
    tx.commit().await?;
    Ok(Json(customer))
}

pub async fn customer_timeline(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<TimelineEvent>>, AppError> {
    let events = state.db.customer_timeline(id).await?;
    Ok(Json(events))
}
