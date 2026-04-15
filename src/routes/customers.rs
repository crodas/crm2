use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::models::customer::*;

#[derive(Deserialize)]
pub struct ListParams {
    pub customer_type_id: Option<i64>,
    pub search: Option<String>,
}

pub async fn list_customer_types(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<CustomerType>>, AppError> {
    let types =
        sqlx::query_as::<_, CustomerType>("SELECT * FROM customer_types ORDER BY sort_order, name")
            .fetch_all(&pool)
            .await?;
    Ok(Json(types))
}

pub async fn reorder_customer_types(
    State(pool): State<SqlitePool>,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<Vec<CustomerType>>, AppError> {
    let mut tx = pool.begin().await?;
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE customer_types SET sort_order = ? WHERE id = ?")
            .bind(i as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    list_customer_types(State(pool)).await
}

pub async fn update_customer_type(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<CustomerType>, AppError> {
    let name = body["name"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("name is required".into()))?;
    let ct = sqlx::query_as::<_, CustomerType>(
        "UPDATE customer_types SET name = ? WHERE id = ? RETURNING *",
    )
    .bind(name)
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Customer type not found".into()))?;
    Ok(Json(ct))
}

pub async fn create_customer_type(
    State(pool): State<SqlitePool>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<CustomerType>, AppError> {
    let name = body["name"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("name is required".into()))?;
    let ct = sqlx::query_as::<_, CustomerType>(
        "INSERT INTO customer_types (name) VALUES (?) RETURNING *",
    )
    .bind(name)
    .fetch_one(&pool)
    .await?;
    Ok(Json(ct))
}

pub async fn list_customers(
    State(pool): State<SqlitePool>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Customer>>, AppError> {
    let customers = if let Some(search) = &params.search {
        let pattern = format!("%{search}%");
        if let Some(type_id) = params.customer_type_id {
            sqlx::query_as::<_, Customer>(
                "SELECT * FROM customers WHERE customer_type_id = ? AND (name LIKE ? OR email LIKE ? OR phone LIKE ?) ORDER BY name",
            )
            .bind(type_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&pool)
            .await?
        } else {
            sqlx::query_as::<_, Customer>(
                "SELECT * FROM customers WHERE name LIKE ? OR email LIKE ? OR phone LIKE ? ORDER BY name",
            )
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&pool)
            .await?
        }
    } else if let Some(type_id) = params.customer_type_id {
        sqlx::query_as::<_, Customer>(
            "SELECT * FROM customers WHERE customer_type_id = ? ORDER BY name",
        )
        .bind(type_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query_as::<_, Customer>("SELECT * FROM customers ORDER BY name")
            .fetch_all(&pool)
            .await?
    };
    Ok(Json(customers))
}

pub async fn get_customer(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<Customer>, AppError> {
    let customer = sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Customer not found".into()))?;
    Ok(Json(customer))
}

pub async fn create_customer(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateCustomer>,
) -> Result<Json<Customer>, AppError> {
    let customer = sqlx::query_as::<_, Customer>(
        "INSERT INTO customers (customer_type_id, name, email, phone, address, notes)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(body.customer_type_id)
    .bind(&body.name)
    .bind(&body.email)
    .bind(&body.phone)
    .bind(&body.address)
    .bind(&body.notes)
    .fetch_one(&pool)
    .await?;
    Ok(Json(customer))
}

pub async fn update_customer(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCustomer>,
) -> Result<Json<Customer>, AppError> {
    // Fetch existing
    let existing = sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE id = ?")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Customer not found".into()))?;

    let customer = sqlx::query_as::<_, Customer>(
        "UPDATE customers SET
            customer_type_id = ?, name = ?, email = ?, phone = ?, address = ?, notes = ?,
            updated_at = datetime('now')
         WHERE id = ? RETURNING *",
    )
    .bind(body.customer_type_id.unwrap_or(existing.customer_type_id))
    .bind(body.name.as_deref().unwrap_or(&existing.name))
    .bind(body.email.as_deref().or(existing.email.as_deref()))
    .bind(body.phone.as_deref().or(existing.phone.as_deref()))
    .bind(body.address.as_deref().or(existing.address.as_deref()))
    .bind(body.notes.as_deref().or(existing.notes.as_deref()))
    .bind(id)
    .fetch_one(&pool)
    .await?;
    Ok(Json(customer))
}

pub async fn customer_timeline(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<TimelineEvent>>, AppError> {
    let events = sqlx::query_as::<_, TimelineEvent>(
        "SELECT 'quote' as event_type, id, title as summary, created_at as date, total_amount as amount
         FROM quotes WHERE customer_id = ?1
         UNION ALL
         SELECT 'sale' as event_type, id, COALESCE(notes, 'Sale') as summary, sold_at as date, total_amount as amount
         FROM sales WHERE customer_id = ?1
         UNION ALL
         SELECT 'booking' as event_type, id, title as summary, start_at as date, NULL as amount
         FROM bookings WHERE customer_id = ?1
         UNION ALL
         SELECT 'payment' as event_type, p.id, ('Payment on: ' || q.title) as summary, p.paid_at as date, p.amount as amount
         FROM payment_utxos p
         JOIN quotes q ON q.id = p.quote_id
         WHERE q.customer_id = ?1
         ORDER BY date DESC",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(events))
}
