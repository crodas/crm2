use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::customer::CustomerGroup;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateGroupReq {
    pub customer_type_id: i64,
    pub default_markup_pct: f64,
}

#[derive(Deserialize)]
pub struct UpdateGroupReq {
    pub default_markup_pct: Option<f64>,
}

pub async fn list_groups(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CustomerGroup>>, AppError> {
    let groups = state.db.list_customer_groups().await?;
    Ok(Json(groups))
}

pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateGroupReq>,
) -> Result<Json<CustomerGroup>, AppError> {
    let type_name = state
        .db
        .get_customer_type_name(body.customer_type_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Customer type not found".into()))?;

    let mut tx = state.db.begin().await?;
    let group = tx
        .create_customer_group(&type_name, body.customer_type_id, body.default_markup_pct)
        .await?;
    tx.commit().await?;
    Ok(Json(group))
}

pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateGroupReq>,
) -> Result<Json<CustomerGroup>, AppError> {
    let mut tx = state.db.begin().await?;
    let existing = tx.get_customer_group(id).await?;
    let group = tx
        .update_customer_group(
            id,
            body.default_markup_pct
                .unwrap_or(existing.default_markup_pct),
        )
        .await?;
    tx.commit().await?;
    Ok(Json(group))
}
