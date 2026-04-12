use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::customer::CustomerGroup;

#[derive(Deserialize)]
pub struct CreateGroupReq {
    pub name: String,
    pub customer_type_id: i64,
    pub default_markup_pct: f64,
}

#[derive(Deserialize)]
pub struct UpdateGroupReq {
    pub name: Option<String>,
    pub default_markup_pct: Option<f64>,
}

pub async fn list_groups(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<CustomerGroup>>, AppError> {
    let groups =
        sqlx::query_as::<_, CustomerGroup>("SELECT * FROM customer_groups ORDER BY name")
            .fetch_all(&pool)
            .await?;
    Ok(Json(groups))
}

pub async fn create_group(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateGroupReq>,
) -> Result<Json<CustomerGroup>, AppError> {
    let group = sqlx::query_as::<_, CustomerGroup>(
        "INSERT INTO customer_groups (name, customer_type_id, default_markup_pct) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(&body.name)
    .bind(body.customer_type_id)
    .bind(body.default_markup_pct)
    .fetch_one(&pool)
    .await?;
    Ok(Json(group))
}

pub async fn update_group(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateGroupReq>,
) -> Result<Json<CustomerGroup>, AppError> {
    let existing =
        sqlx::query_as::<_, CustomerGroup>("SELECT * FROM customer_groups WHERE id = ?")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Customer group not found".into()))?;

    let group = sqlx::query_as::<_, CustomerGroup>(
        "UPDATE customer_groups SET name = ?, default_markup_pct = ? WHERE id = ? RETURNING *",
    )
    .bind(body.name.as_deref().unwrap_or(&existing.name))
    .bind(body.default_markup_pct.unwrap_or(existing.default_markup_pct))
    .bind(id)
    .fetch_one(&pool)
    .await?;
    Ok(Json(group))
}
