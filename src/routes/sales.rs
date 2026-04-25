use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::error::AppError;
use crate::models::sale::*;
use crate::state::AppState;

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

    let mut tx = state.db.begin().await?;
    let sale = tx
        .create_sale(
            body.customer_id,
            body.customer_group_id,
            body.notes.as_deref(),
            &lines,
            body.payment_method.as_deref(),
        )
        .await?;
    tx.commit().await?;
    Ok(Json(sale))
}

pub async fn list_sales(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Sale>>, AppError> {
    let sales = state.db.list_sales().await?;
    Ok(Json(sales))
}

pub async fn get_sale(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sale = state.db.get_sale(id).await?;
    let lines = state.db.get_sale_lines(id).await?;
    let payments = state.db.get_sale_payments(id).await?;

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
    let sale = state.db.get_sale(sale_id).await?;

    if body.amount.0 <= 0 {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    let mut tx = state.db.begin().await?;
    let payment = tx.record_sale_payment(&sale, &body).await?;
    tx.commit().await?;
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

        let ledger_pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let storage = ledger_sqlite::SqliteStorage::from_pool(ledger_pool)
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

        let db = crate::storage::Db::new(pool, ledger, "1".into());

        // Seed stock: 100 units in warehouse 1
        let product_asset = db.asset("product:1").unwrap();
        let seed_amount = product_asset.parse_amount("100.000").unwrap();
        let tx = db
            .ledger()
            .transaction("seed-stock")
            .issue("warehouse/1", &seed_amount)
            .unwrap()
            .build()
            .await
            .unwrap();
        db.ledger().commit(tx).await.unwrap();

        let state = Arc::new(AppState { db });

        let app = Router::new()
            .route("/sales", get(list_sales).post(create_sale))
            .route("/sales/{id}", get(get_sale))
            .route("/sales/{id}/payments", post(record_sale_payment))
            .with_state(state.clone());

        (app, state)
    }

    fn sale_request(payment_method: Option<&str>) -> Request<Body> {
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
        assert_eq!(sale["total_amount"], 100.0);

        let bal = state.db.ledger().balance("customer/1", "gs").await.unwrap();
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

        let bal = state.db.ledger().balance("customer/1/debt", "gs").await.unwrap();
        assert_eq!(bal, 0);

        let cash_bal = state.db.ledger().balance("warehouse/cash", "gs").await.unwrap();
        assert_eq!(cash_bal, 10000);
    }

    #[tokio::test]
    async fn record_payment_on_credit_sale() {
        let (app, state) = setup().await;

        let resp = app.clone().oneshot(sale_request(None)).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let sale: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let sale_id = sale["id"].as_i64().unwrap();

        // Record partial payment: 60.0
        let pay_req = Request::builder()
            .method("POST")
            .uri(format!("/sales/{sale_id}/payments"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "amount": 60.0, "method": "cash" }).to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(pay_req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let get_req = Request::builder()
            .uri(format!("/sales/{sale_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(get_req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail["total_paid"], 6000);
        assert_eq!(detail["balance"], 4000);
        assert_eq!(detail["sale"]["payment_status"], "credit");

        // Record remaining payment: 40.0
        let pay_req = Request::builder()
            .method("POST")
            .uri(format!("/sales/{sale_id}/payments"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "amount": 40.0, "method": "transfer" }).to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(pay_req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let get_req = Request::builder()
            .uri(format!("/sales/{sale_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(get_req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail["balance"], 0);
        assert_eq!(detail["sale"]["payment_status"], "paid");

        let bal = state.db.ledger().balance("customer/1/debt", "gs").await.unwrap();
        assert_eq!(bal, 0);

        let cash_bal = state.db.ledger().balance("warehouse/cash", "gs").await.unwrap();
        assert_eq!(cash_bal, 10000);
    }
}
