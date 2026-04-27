use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::amount::Amount;
use crate::error::AppError;
use crate::models::inventory::*;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct StockQuery {
    pub product_id: Option<i64>,
    pub warehouse_id: Option<i64>,
}

pub async fn receive_inventory(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ReceiveInventoryRequest>,
) -> Result<Json<InventoryReceipt>, AppError> {
    let mut tx = state.db.begin().await?;
    let receipt = tx.receive_inventory(&body).await?;
    tx.commit().await?;
    Ok(Json(receipt))
}

pub async fn get_stock(
    State(state): State<Arc<AppState>>,
    Query(params): Query<StockQuery>,
) -> Result<Json<Vec<StockLevel>>, AppError> {
    let stock = state
        .db
        .stock_levels(params.product_id, params.warehouse_id)
        .await?;
    Ok(Json(stock))
}

pub async fn list_receipts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InventoryReceipt>>, AppError> {
    let receipts = state.db.list_receipts().await?;
    Ok(Json(receipts))
}

pub async fn get_receipt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let receipt = state.db.get_receipt(id).await?;
    let lines = state.db.get_receipt_lines(id).await?;
    let prices = state.db.get_receipt_prices(id).await?;
    let ledger_entries = state.db.get_supplier_entries(id).await?;
    let outstanding = state.db.receipt_outstanding(id).await.unwrap_or(0);

    let total_paid: i64 = ledger_entries
        .iter()
        .filter(|e| e.amount.cents() > 0)
        .map(|e| e.amount.cents())
        .sum();

    Ok(Json(serde_json::json!({
        "receipt": receipt,
        "lines": lines,
        "prices": prices,
        "ledger": ledger_entries,
        "total_paid": Amount(total_paid),
        "balance": Amount(outstanding as i64),
    })))
}

pub async fn latest_prices(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LatestPriceQuery>,
) -> Result<Json<Vec<LatestPrice>>, AppError> {
    let prices = state.db.latest_prices(&params).await?;
    Ok(Json(prices))
}

pub async fn record_supplier_payment(
    State(state): State<Arc<AppState>>,
    Path(receipt_id): Path<i64>,
    Json(body): Json<CreateSupplierPayment>,
) -> Result<Json<SupplierLedgerUtxo>, AppError> {
    // Verify receipt exists
    let _receipt = state.db.get_receipt(receipt_id).await?;

    if body.amount.cents() <= 0 {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    let mut tx = state.db.begin().await?;
    let entry = tx.record_supplier_payment(receipt_id, &body).await?;
    tx.commit().await?;
    Ok(Json(entry))
}

pub async fn list_transfers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let txs = state.db.ledger_transactions().await?;

    let transfers: Vec<serde_json::Value> = txs
        .iter()
        .filter(|tx| tx.idempotency_key.starts_with("transfer-"))
        .map(|tx| {
            let parts: Vec<&str> = tx.idempotency_key.splitn(4, '-').collect();
            let from_warehouse_id: i64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let to_warehouse_id: i64 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let timestamp_ms: i64 = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);

            let lines: Vec<serde_json::Value> = tx
                .credits
                .iter()
                .filter(|c| {
                    c.amount.asset_name().starts_with("product:")
                        && c.to.as_str() == format!("warehouse/{to_warehouse_id}")
                })
                .filter_map(|c| {
                    let product_id: i64 = c
                        .amount
                        .asset_name()
                        .strip_prefix("product:")?
                        .parse()
                        .ok()?;
                    let qty: f64 = c.amount.to_string().parse().ok()?;
                    Some(serde_json::json!({
                        "product_id": product_id,
                        "quantity": qty,
                    }))
                })
                .collect();

            serde_json::json!({
                "tx_id": tx.tx_id,
                "from_warehouse_id": from_warehouse_id,
                "to_warehouse_id": to_warehouse_id,
                "timestamp_ms": timestamp_ms,
                "lines": lines,
            })
        })
        .rev()
        .collect();

    Ok(Json(transfers))
}

pub async fn transfer_inventory(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TransferInventoryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut tx = state.db.begin().await?;
    let tx_id = tx.transfer_inventory(&body).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "ok": true, "tx_id": tx_id })))
}

pub async fn supplier_balance(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SupplierBalance>, AppError> {
    let balance = state.db.supplier_balance().await?;
    Ok(Json(balance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use http_body_util::BodyExt;
    use ledger::debt::SignedPositionDebt;
    use ledger::Asset;
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    async fn setup() -> (Router, Arc<AppState>) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::init_pool_with(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO warehouses (id, name) VALUES (1, 'Warehouse A'), (2, 'Warehouse B')",
        )
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
        let state = Arc::new(AppState { db });

        let app = Router::new()
            .route(
                "/inventory/transfers",
                get(list_transfers).post(transfer_inventory),
            )
            .with_state(state.clone());

        (app, state)
    }

    async fn seed_stock(state: &AppState, warehouse_id: i64, product_id: i64, qty: f64) {
        let account = format!("warehouse/{warehouse_id}");
        let asset = state.db.asset(&format!("product:{product_id}")).unwrap();
        let amount = asset.parse_amount(&format!("{qty:.3}")).unwrap();
        let tx = state
            .db
            .ledger()
            .transaction(format!("seed-{warehouse_id}-{product_id}"))
            .issue(&account, &amount)
            .unwrap()
            .build()
            .await
            .unwrap();
        state.db.ledger().commit(tx).await.unwrap();
    }

    fn transfer_request(from: i64, to: i64, product_id: i64, qty: f64) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/inventory/transfers")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "from_warehouse_id": from,
                    "to_warehouse_id": to,
                    "lines": [{ "product_id": product_id, "quantity": qty }]
                })
                .to_string(),
            ))
            .unwrap()
    }

    #[tokio::test]
    async fn transfer_moves_stock_between_warehouses() {
        let (app, state) = setup().await;
        seed_stock(&state, 1, 1, 10.0).await;

        let resp = app.oneshot(transfer_request(1, 2, 1, 4.0)).await.unwrap();
        assert_eq!(resp.status(), 200);

        let from_bal = state
            .db
            .ledger()
            .balance("warehouse/1", "product:1")
            .await
            .unwrap();
        let to_bal = state
            .db
            .ledger()
            .balance("warehouse/2", "product:1")
            .await
            .unwrap();

        assert_eq!(from_bal, 6000);
        assert_eq!(to_bal, 4000);
    }

    #[tokio::test]
    async fn transfer_rejects_same_warehouse() {
        let (app, _state) = setup().await;

        let resp = app.oneshot(transfer_request(1, 1, 1, 1.0)).await.unwrap();
        assert_eq!(resp.status(), 400);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("different"));
    }

    #[tokio::test]
    async fn transfer_rejects_empty_lines() {
        let (app, _state) = setup().await;

        let req = Request::builder()
            .method("POST")
            .uri("/inventory/transfers")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "from_warehouse_id": 1,
                    "to_warehouse_id": 2,
                    "lines": []
                })
                .to_string(),
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn transfer_rejects_zero_quantity() {
        let (app, _state) = setup().await;

        let resp = app.oneshot(transfer_request(1, 2, 1, 0.0)).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn transfer_rejects_negative_quantity() {
        let (app, _state) = setup().await;

        let resp = app.oneshot(transfer_request(1, 2, 1, -5.0)).await.unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn list_transfers_returns_completed_transfers() {
        let (app, state) = setup().await;
        seed_stock(&state, 1, 1, 10.0).await;

        let resp = app
            .clone()
            .oneshot(transfer_request(1, 2, 1, 3.0))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let req = Request::builder()
            .method("GET")
            .uri("/inventory/transfers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["from_warehouse_id"], 1);
        assert_eq!(json[0]["to_warehouse_id"], 2);
        assert_eq!(json[0]["lines"][0]["product_id"], 1);
        assert_eq!(json[0]["lines"][0]["quantity"], 3.0);
    }

    #[tokio::test]
    async fn list_transfers_empty_when_no_transfers() {
        let (app, _state) = setup().await;

        let req = Request::builder()
            .method("GET")
            .uri("/inventory/transfers")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(json.is_empty());
    }
}
