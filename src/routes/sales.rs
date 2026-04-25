use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::sale::*;
use crate::state::AppState;

/// Core sale creation logic, usable from both the HTTP handler and tests.
/// Each tuple in `lines` is `(product_id, warehouse_id, quantity, price_per_unit_cents)`.
/// If `payment_method` is Some, the sale is paid immediately; otherwise it's deferred (credit).
pub async fn create_sale_tx(
    state: &AppState,
    customer_id: i64,
    customer_group_id: i64,
    notes: Option<&str>,
    lines: &[(i64, i64, f64, i64)],
    payment_method: Option<&str>,
) -> Result<Sale, AppError> {
    let total: Amount = lines
        .iter()
        .map(|&(_, _, qty, price)| Amount(price).mul_qty(qty))
        .sum();

    let payment_status = if payment_method.is_some() {
        "paid"
    } else {
        "credit"
    };

    // Insert sale metadata
    let sale = sqlx::query_as::<_, Sale>(
        "INSERT INTO sales (customer_id, customer_group_id, notes, total_amount, payment_status)
         VALUES (?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(customer_id)
    .bind(customer_group_id)
    .bind(notes)
    .bind(total)
    .bind(payment_status)
    .fetch_one(&state.pool)
    .await?;

    // Insert sale lines
    for &(product_id, _warehouse_id, quantity, price_cents) in lines {
        if quantity <= 0.0 {
            return Err(AppError::BadRequest("Quantity must be positive".into()));
        }

        sqlx::query(
            "INSERT INTO sale_lines (sale_id, product_id, quantity, price_per_unit)
             VALUES (?, ?, ?, ?)",
        )
        .bind(sale.id)
        .bind(product_id)
        .bind(quantity)
        .bind(price_cents)
        .execute(&state.pool)
        .await?;
    }

    // Build ledger transaction: debit inventory, credit sold sink + customer debt
    let mut builder = state.ledger.transaction(format!("sale-{}", sale.id));

    for &(product_id, warehouse_id, quantity, _price_cents) in lines {
        let account = format!("warehouse/{warehouse_id}");
        let asset = state
            .ledger
            .asset(&format!("product:{product_id}"))
            .ok_or_else(|| {
                AppError::Internal(format!("asset product:{product_id} not registered"))
            })?;
        let amount = asset
            .parse_amount(&format!("{quantity:.3}"))
            .map_err(|e| AppError::Internal(format!("parse amount: {e}")))?;

        builder = builder
            .debit(&account, &amount)
            .credit(&format!("customer/{customer_id}"), &amount);
    }

    let gs = state
        .ledger
        .asset("gs")
        .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;

    if payment_method.is_none() {
        // Deferred (credit): issue debt via configured DebtStrategy
        let debt_amount = gs
            .try_amount(total.cents().into())
            .map_err(|e| AppError::Internal(format!("gs amount: {e}")))?;
        builder = builder
            .create_debt(&customer_id.to_string(), &state.store_id, &debt_amount)
            .map_err(|e| AppError::Internal(format!("create debt: {e}")))?;
    }

    let ledger_tx = builder.build().await.map_err(|e| match e {
        ledger::Error::InsufficientBalance {
            account,
            asset: _,
            required,
            available,
        } => {
            // Parse product_id from account "warehouse/{wh}"
            let product_id = account
                .split('/')
                .last()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let asset_obj = state.ledger.asset(&format!("product:{product_id}"));
            let divisor = asset_obj
                .map(|a| 10_f64.powi(a.precision() as i32))
                .unwrap_or(1000.0);
            AppError::InsufficientStock {
                product_id,
                requested: required as f64 / divisor,
                available: available as f64 / divisor,
            }
        }
        other => AppError::Internal(format!("ledger build: {other}")),
    })?;

    state
        .ledger
        .commit(ledger_tx)
        .await
        .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

    // If paid immediately: credit cash in a separate ledger tx + record payment
    if let Some(method) = payment_method {
        let cash_amount = gs
            .try_amount(total.cents().into())
            .map_err(|e| AppError::Internal(format!("gs amount: {e}")))?;
        let cash_tx = state
            .ledger
            .transaction(format!("sale-{}-cash", sale.id))
            .issue("warehouse/cash", &cash_amount)
            .map_err(|e| AppError::Internal(format!("issue: {e}")))?
            .build()
            .await
            .map_err(|e| AppError::Internal(format!("cash ledger build: {e}")))?;
        state
            .ledger
            .commit(cash_tx)
            .await
            .map_err(|e| AppError::Internal(format!("cash ledger commit: {e}")))?;

        sqlx::query("INSERT INTO sale_payments (sale_id, amount, method) VALUES (?, ?, ?)")
            .bind(sale.id)
            .bind(total)
            .bind(method)
            .execute(&state.pool)
            .await?;
    }

    Ok(sale)
}

pub async fn create_sale(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSaleRequest>,
) -> Result<Json<Sale>, AppError> {
    let lines: Vec<(i64, i64, f64, i64)> = body
        .lines
        .iter()
        .map(|l| {
            (
                l.product_id,
                l.warehouse_id,
                l.quantity,
                l.price_per_unit.cents(),
            )
        })
        .collect();

    let sale = create_sale_tx(
        &state,
        body.customer_id,
        body.customer_group_id,
        body.notes.as_deref(),
        &lines,
        body.payment_method.as_deref(),
    )
    .await?;
    Ok(Json(sale))
}

pub async fn list_sales(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Sale>>, AppError> {
    let sales = sqlx::query_as::<_, Sale>("SELECT * FROM sales ORDER BY sold_at DESC")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(sales))
}

pub async fn get_sale(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sale = sqlx::query_as::<_, Sale>("SELECT * FROM sales WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Sale not found".into()))?;

    let lines = sqlx::query_as::<_, SaleLine>("SELECT * FROM sale_lines WHERE sale_id = ?")
        .bind(id)
        .fetch_all(&state.pool)
        .await?;

    let payments =
        sqlx::query_as::<_, SalePayment>("SELECT * FROM sale_payments WHERE sale_id = ?")
            .bind(id)
            .fetch_all(&state.pool)
            .await?;

    let total_paid: i64 = payments.iter().map(|p| p.amount.cents()).sum();
    let balance = sale.total_amount.cents() - total_paid;

    Ok(Json(serde_json::json!({
        "sale": sale,
        "lines": lines,
        "payments": payments,
        "total_paid": total_paid,
        "balance": balance,
    })))
}

pub async fn record_sale_payment(
    State(state): State<Arc<AppState>>,
    Path(sale_id): Path<i64>,
    Json(body): Json<CreateSalePayment>,
) -> Result<Json<SalePayment>, AppError> {
    let sale = sqlx::query_as::<_, Sale>("SELECT * FROM sales WHERE id = ?")
        .bind(sale_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Sale not found".into()))?;

    if body.amount.0 <= 0 {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    // Insert payment record
    let payment = sqlx::query_as::<_, SalePayment>(
        "INSERT INTO sale_payments (sale_id, amount, method, notes) VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(sale_id)
    .bind(body.amount)
    .bind(&body.method)
    .bind(&body.notes)
    .fetch_one(&state.pool)
    .await?;

    // Settle debt in the ledger
    let customer_id = sale.customer_id;
    let amount: i128 = body.amount.cents().into();

    let gs = state
        .ledger
        .asset("gs")
        .ok_or_else(|| AppError::Internal("gs asset not registered".into()))?;

    let gs_amount = gs
        .try_amount(amount)
        .map_err(|e| AppError::Internal(format!("gs amount: {e}")))?;
    let ledger_tx = state
        .ledger
        .transaction(format!("sale-payment-{}", payment.id))
        .settle_debt(&customer_id.to_string(), &state.store_id, &gs_amount)
        .await
        .map_err(|e| AppError::Internal(format!("settle debt: {e}")))?
        .issue("warehouse/cash", &gs_amount)
        .map_err(|e| AppError::Internal(format!("issue cash: {e}")))?
        .build()
        .await
        .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
    state
        .ledger
        .commit(ledger_tx)
        .await
        .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

    // Update payment_status if fully paid
    let total_paid: Amount =
        sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM sale_payments WHERE sale_id = ?")
            .bind(sale_id)
            .fetch_one(&state.pool)
            .await?;

    if total_paid.cents() >= sale.total_amount.cents() {
        sqlx::query("UPDATE sales SET payment_status = 'paid' WHERE id = ?")
            .bind(sale_id)
            .execute(&state.pool)
            .await?;
    }

    Ok(Json(payment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::Request,
        routing::{get, post},
        Router,
    };
    use http_body_util::BodyExt;
    use ledger::debt::SignedPositionDebt;
    use ledger::Asset;
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    async fn setup() -> (Router, Arc<AppState>) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::init_pool_with(&pool).await.unwrap();

        // customer_types and customer_groups already seeded by migrations
        sqlx::query("INSERT INTO customers (id, name, customer_type_id) VALUES (1, 'Alice', 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO warehouses (id, name) VALUES (1, 'Main')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO products (id, name, product_type) VALUES (1, 'Widget', 'product')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let storage = ledger_sqlite::SqliteStorage::from_pool(pool.clone())
            .await
            .unwrap();
        let ledger = ledger::Ledger::new(Arc::new(storage)).with_debt_strategy(
            SignedPositionDebt::new("customer/{from}", "warehouse/{to}/receivables/{from}"),
        );
        ledger.register_asset(Asset::new("gs", 0)).await.unwrap();
        ledger
            .register_asset(Asset::new("product:1", 3))
            .await
            .unwrap();

        let state = Arc::new(AppState {
            pool,
            ledger,
            store_id: "1".into(),
        });

        // Seed stock: 100 units in warehouse 1 (precision=3, so "100.000")
        let product_asset = state.ledger.asset("product:1").unwrap();
        let seed_amount = product_asset.parse_amount("100.000").unwrap();
        let tx = state
            .ledger
            .transaction("seed-stock")
            .issue("warehouse/1", &seed_amount)
            .unwrap()
            .build()
            .await
            .unwrap();
        state.ledger.commit(tx).await.unwrap();

        let app = Router::new()
            .route("/sales", get(list_sales).post(create_sale))
            .route("/sales/{id}", get(get_sale))
            .route("/sales/{id}/payments", post(record_sale_payment))
            .with_state(state.clone());

        (app, state)
    }

    fn sale_request(payment_method: Option<&str>) -> Request<Body> {
        // price_per_unit: 50.0 (50 currency units = 5000 cents)
        // quantity: 2 → total = 100.0 (10000 cents)
        let mut body = serde_json::json!({
            "customer_id": 1,
            "customer_group_id": 1,
            "lines": [{
                "product_id": 1,
                "warehouse_id": 1,
                "quantity": 2.0,
                "price_per_unit": 50.0
            }]
        });
        if let Some(m) = payment_method {
            body["payment_method"] = serde_json::Value::String(m.to_string());
        }
        Request::builder()
            .method("POST")
            .uri("/sales")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn create_sale_credit_creates_debt() {
        let (app, state) = setup().await;

        let resp = app.oneshot(sale_request(None)).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let sale: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(sale["payment_status"], "credit");
        // total = 2 * 50.0 = 100.0 currency units (serialized as float)
        assert_eq!(sale["total_amount"], 100.0);

        // Customer should have debt in the ledger (10000 cents)
        let bal = state.ledger.balance("customer/1", "gs").await.unwrap();
        assert_eq!(bal, -10000);
    }

    #[tokio::test]
    async fn create_sale_paid_credits_cash() {
        let (app, state) = setup().await;

        let resp = app.oneshot(sale_request(Some("cash"))).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let sale: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(sale["payment_status"], "paid");

        // No debt issued for paid sales
        let bal = state.ledger.balance("customer/1/debt", "gs").await.unwrap();
        assert_eq!(bal, 0);

        // Cash account should be credited (10000 cents)
        let cash_bal = state.ledger.balance("warehouse/cash", "gs").await.unwrap();
        assert_eq!(cash_bal, 10000);
    }

    #[tokio::test]
    async fn record_payment_on_credit_sale() {
        let (app, state) = setup().await;

        // Create a credit sale (total = 100.0 = 10000 cents)
        let resp = app.clone().oneshot(sale_request(None)).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let sale: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let sale_id = sale["id"].as_i64().unwrap();

        // Record partial payment: 60.0 currency units
        let pay_req = Request::builder()
            .method("POST")
            .uri(format!("/sales/{sale_id}/payments"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "amount": 60.0,
                    "method": "cash"
                })
                .to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(pay_req).await.unwrap();
        assert_eq!(resp.status(), 200);

        // Check sale detail — still credit (partial payment)
        let get_req = Request::builder()
            .uri(format!("/sales/{sale_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(get_req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // total_paid and balance are returned as raw cents from get_sale
        assert_eq!(detail["total_paid"], 6000);
        assert_eq!(detail["balance"], 4000);
        assert_eq!(detail["sale"]["payment_status"], "credit");

        // Record remaining payment: 40.0 currency units
        let pay_req = Request::builder()
            .method("POST")
            .uri(format!("/sales/{sale_id}/payments"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "amount": 40.0,
                    "method": "transfer"
                })
                .to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(pay_req).await.unwrap();
        assert_eq!(resp.status(), 200);

        // Now should be fully paid
        let get_req = Request::builder()
            .uri(format!("/sales/{sale_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(get_req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail["balance"], 0);
        assert_eq!(detail["sale"]["payment_status"], "paid");

        // Ledger should show zero debt (fully settled)
        let bal = state.ledger.balance("customer/1/debt", "gs").await.unwrap();
        assert_eq!(bal, 0);

        // Cash should have full amount (10000 cents)
        let cash_bal = state.ledger.balance("warehouse/cash", "gs").await.unwrap();
        assert_eq!(cash_bal, 10000);
    }
}
