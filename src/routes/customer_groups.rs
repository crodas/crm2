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
    let groups = sqlx::query_as::<_, CustomerGroup>("SELECT * FROM customer_groups ORDER BY id")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(groups))
}

pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateGroupReq>,
) -> Result<Json<CustomerGroup>, AppError> {
    // Auto-derive name from customer type
    let type_name: String = sqlx::query_scalar("SELECT name FROM customer_types WHERE id = ?")
        .bind(body.customer_type_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::BadRequest("Customer type not found".into()))?;

    let group = sqlx::query_as::<_, CustomerGroup>(
        "INSERT INTO customer_groups (name, customer_type_id, default_markup_pct) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(&type_name)
    .bind(body.customer_type_id)
    .bind(body.default_markup_pct)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(group))
}

pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateGroupReq>,
) -> Result<Json<CustomerGroup>, AppError> {
    let existing = sqlx::query_as::<_, CustomerGroup>("SELECT * FROM customer_groups WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Customer group not found".into()))?;

    let group = sqlx::query_as::<_, CustomerGroup>(
        "UPDATE customer_groups SET default_markup_pct = ? WHERE id = ? RETURNING *",
    )
    .bind(
        body.default_markup_pct
            .unwrap_or(existing.default_markup_pct),
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(group))
}
