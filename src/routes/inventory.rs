use axum::{
    extract::{Path, Query, State},
    Json,
};
use ledger::AccountPath;
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
    let mut tx = state.pool.begin().await?;

    // Calculate total_cost from lines
    let total_cost: i64 = body
        .lines
        .iter()
        .map(|l| (l.quantity * l.cost_per_unit.cents() as f64).round() as i64)
        .sum();

    let receipt = sqlx::query_as::<_, InventoryReceipt>(
        "INSERT INTO inventory_receipts (reference, supplier_name, notes, total_cost)
         VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(&body.reference)
    .bind(&body.supplier_name)
    .bind(&body.notes)
    .bind(total_cost)
    .fetch_one(&mut *tx)
    .await?;

    // Build a ledger transaction to issue inventory tokens
    let mut builder = state.ledger.transaction(format!("receipt-{}", receipt.id));

    for line in &body.lines {
        if line.quantity <= 0.0 {
            return Err(AppError::BadRequest("Quantity must be positive".into()));
        }

        // Store receipt line item metadata
        sqlx::query(
            "INSERT INTO inventory_receipt_lines (receipt_id, product_id, warehouse_id, quantity, cost_per_unit)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(receipt.id)
        .bind(line.product_id)
        .bind(line.warehouse_id)
        .bind(line.quantity)
        .bind(line.cost_per_unit)
        .execute(&mut *tx)
        .await?;

        // Credit inventory to the store warehouse
        let account = format!("@store/{}/product/{}", line.warehouse_id, line.product_id);
        let asset = format!("product:{}", line.product_id);
        let qty = format!("{:.3}", line.quantity);
        builder = builder.credit(&account, &asset, &qty);

        // Store prices for each customer group
        for price in &line.prices {
            sqlx::query(
                "INSERT INTO inventory_receipt_prices (receipt_id, product_id, customer_group_id, price_per_unit)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(receipt.id)
            .bind(line.product_id)
            .bind(price.customer_group_id)
            .bind(price.price_per_unit)
            .execute(&mut *tx)
            .await?;
        }
    }

    // Handle supplier debt
    let is_credit = body.is_credit.unwrap_or(false);
    let paid_cash = body.paid_cash.unwrap_or(false);

    if is_credit || paid_cash {
        let total_str = format!("{total_cost}");
        let neg_total_str = format!("-{total_cost}");

        // Debt: supplier is owed, store has payable
        builder = builder
            .credit(&format!("@supplier/{}", receipt.id), "gs", &neg_total_str)
            .credit(&format!("@store/payables/{}", receipt.id), "gs", &total_str);

        // Record metadata for the debt entry
        sqlx::query(
            "INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, notes) VALUES (?, ?, ?, ?)",
        )
        .bind(receipt.id)
        .bind(-total_cost)
        .bind::<Option<String>>(None)
        .bind("Inventory received")
        .execute(&mut *tx)
        .await?;

        if paid_cash {
            // Settle immediately: cancel the debt
            builder = builder
                .credit(&format!("@supplier/{}", receipt.id), "gs", &total_str)
                .credit(
                    &format!("@store/payables/{}", receipt.id),
                    "gs",
                    &neg_total_str,
                );

            // Record metadata for the payment entry
            sqlx::query(
                "INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, notes) VALUES (?, ?, ?, ?)",
            )
            .bind(receipt.id)
            .bind(total_cost)
            .bind("cash")
            .bind("Paid in cash")
            .execute(&mut *tx)
            .await?;
        }
    }

    // Commit SQL metadata first, then ledger transaction
    tx.commit().await?;

    let ledger_tx = builder
        .build()
        .await
        .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
    state
        .ledger
        .commit(ledger_tx)
        .await
        .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

    Ok(Json(receipt))
}

pub async fn get_stock(
    State(state): State<Arc<AppState>>,
    Query(params): Query<StockQuery>,
) -> Result<Json<Vec<StockLevel>>, AppError> {
    let prefix = AccountPath::new("@store").map_err(|e| AppError::Internal(e.to_string()))?;
    let entries = state
        .ledger
        .balances_by_prefix(&prefix)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Parse account paths like "@store/{warehouse_id}/product/{product_id}"
    // and filter to product assets only
    let stock: Vec<StockLevel> = entries
        .iter()
        .filter(|e| e.asset_name.starts_with("product:"))
        .filter_map(|e| {
            let path = e.account.as_str();
            let parts: Vec<&str> = path.split('/').collect();
            // Expected: ["@store", "{wh_id}", "product", "{pid}"]
            if parts.len() != 4 || parts[2] != "product" {
                return None;
            }
            let warehouse_id: i64 = parts[1].parse().ok()?;
            let product_id: i64 = parts[3].parse().ok()?;

            // Apply filters
            if let Some(pid) = params.product_id {
                if product_id != pid {
                    return None;
                }
            }
            if let Some(wid) = params.warehouse_id {
                if warehouse_id != wid {
                    return None;
                }
            }

            // Get the asset's precision to convert i128 back to f64
            let asset = state.ledger.asset(&e.asset_name)?;
            let precision = asset.precision() as u32;
            let divisor = 10_f64.powi(precision as i32);
            let total_quantity = e.balance as f64 / divisor;

            Some(StockLevel {
                product_id,
                warehouse_id,
                total_quantity,
            })
        })
        .collect();

    Ok(Json(stock))
}

pub async fn list_receipts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InventoryReceipt>>, AppError> {
    let receipts = sqlx::query_as::<_, InventoryReceipt>(
        "SELECT * FROM inventory_receipts ORDER BY received_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(receipts))
}

pub async fn get_receipt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let receipt =
        sqlx::query_as::<_, InventoryReceipt>("SELECT * FROM inventory_receipts WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Receipt not found".into()))?;

    let lines = sqlx::query_as::<_, ReceiptLine>(
        "SELECT * FROM inventory_receipt_lines WHERE receipt_id = ?",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let prices = sqlx::query_as::<_, ReceiptPrice>(
        "SELECT * FROM inventory_receipt_prices WHERE receipt_id = ?",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    // Supplier ledger metadata (method, notes)
    let ledger_entries = sqlx::query_as::<_, SupplierLedgerUtxo>(
        "SELECT * FROM supplier_ledger_utxos WHERE receipt_id = ? ORDER BY id ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    // Compute supplier balance from the ledger
    let payable_account = format!("@store/payables/{id}");
    let outstanding = match AccountPath::new(&payable_account) {
        Ok(acc) => state.ledger.balance(&acc, "gs").await.unwrap_or(0),
        Err(_) => 0,
    };

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

/// Returns the latest receipt price for each product per customer group.
pub async fn latest_prices(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LatestPriceQuery>,
) -> Result<Json<Vec<LatestPrice>>, AppError> {
    let mut where_clauses = Vec::new();
    if params.product_id.is_some() {
        where_clauses.push("product_id = ?");
    }
    if params.customer_group_id.is_some() {
        where_clauses.push("customer_group_id = ?");
    }
    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!(
        "SELECT p.product_id, p.customer_group_id, p.price_per_unit
         FROM inventory_receipt_prices p
         INNER JOIN (
             SELECT product_id, customer_group_id, MAX(receipt_id) as max_receipt_id
             FROM inventory_receipt_prices
             {where_sql}
             GROUP BY product_id, customer_group_id
         ) latest ON p.product_id = latest.product_id
                  AND p.customer_group_id = latest.customer_group_id
                  AND p.receipt_id = latest.max_receipt_id"
    );

    let mut query = sqlx::query_as::<_, LatestPrice>(&sql);
    if let Some(pid) = params.product_id {
        query = query.bind(pid);
    }
    if let Some(gid) = params.customer_group_id {
        query = query.bind(gid);
    }

    let prices = query.fetch_all(&state.pool).await?;
    Ok(Json(prices))
}

pub async fn record_supplier_payment(
    State(state): State<Arc<AppState>>,
    Path(receipt_id): Path<i64>,
    Json(body): Json<CreateSupplierPayment>,
) -> Result<Json<SupplierLedgerUtxo>, AppError> {
    let _receipt =
        sqlx::query_as::<_, InventoryReceipt>("SELECT * FROM inventory_receipts WHERE id = ?")
            .bind(receipt_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Receipt not found".into()))?;

    if body.amount.cents() <= 0 {
        return Err(AppError::BadRequest("Amount must be positive".into()));
    }

    let amount = body.amount.cents();
    let amount_str = format!("{amount}");
    let neg_amount_str = format!("-{amount}");

    // Record metadata
    let entry = sqlx::query_as::<_, SupplierLedgerUtxo>(
        "INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, notes) VALUES (?, ?, ?, ?) RETURNING *",
    )
    .bind(receipt_id)
    .bind(body.amount)
    .bind(&body.method)
    .bind(&body.notes)
    .fetch_one(&state.pool)
    .await?;

    // Record in ledger: reduce supplier debt and our payable
    let ledger_tx = state
        .ledger
        .transaction(format!("supplier-payment-{}", entry.id))
        .credit(&format!("@supplier/{receipt_id}"), "gs", &amount_str)
        .credit(
            &format!("@store/payables/{receipt_id}"),
            "gs",
            &neg_amount_str,
        )
        .build()
        .await
        .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
    state
        .ledger
        .commit(ledger_tx)
        .await
        .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

    Ok(Json(entry))
}

pub async fn transfer_inventory(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TransferInventoryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if body.from_warehouse_id == body.to_warehouse_id {
        return Err(AppError::BadRequest(
            "Source and destination warehouse must be different".into(),
        ));
    }
    if body.lines.is_empty() {
        return Err(AppError::BadRequest("At least one line is required".into()));
    }

    // Verify warehouses exist
    let from_wh = sqlx::query_scalar::<_, i64>("SELECT id FROM warehouses WHERE id = ?")
        .bind(body.from_warehouse_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Source warehouse not found".into()))?;

    let to_wh = sqlx::query_scalar::<_, i64>("SELECT id FROM warehouses WHERE id = ?")
        .bind(body.to_warehouse_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Destination warehouse not found".into()))?;

    let tx_id = format!(
        "transfer-{}-{}-{}",
        from_wh,
        to_wh,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let mut builder = state.ledger.transaction(&tx_id);

    for line in &body.lines {
        if line.quantity <= 0.0 {
            return Err(AppError::BadRequest("Quantity must be positive".into()));
        }

        let asset = format!("product:{}", line.product_id);
        let qty = format!("{:.3}", line.quantity);

        let from_account = format!("@store/{}/product/{}", from_wh, line.product_id);
        let to_account = format!("@store/{}/product/{}", to_wh, line.product_id);

        // Debit source (spend UTXO via FIFO), credit destination
        builder = builder
            .debit(&from_account, &asset, &qty)
            .credit(&to_account, &asset, &qty);
    }

    let ledger_tx = builder
        .build()
        .await
        .map_err(|e| AppError::Internal(format!("ledger build: {e}")))?;
    state
        .ledger
        .commit(ledger_tx)
        .await
        .map_err(|e| AppError::Internal(format!("ledger commit: {e}")))?;

    Ok(Json(serde_json::json!({ "ok": true, "tx_id": tx_id })))
}

pub async fn supplier_balance(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SupplierBalance>, AppError> {
    let prefix =
        AccountPath::new("@store/payables").map_err(|e| AppError::Internal(e.to_string()))?;
    let tokens = state
        .ledger
        .unspent_tokens_prefix(&prefix, "gs")
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let total_owed: i64 = tokens
        .iter()
        .filter(|t| t.qty > 0)
        .map(|t| t.qty as i64)
        .sum();
    let total_paid_offset: i64 = tokens
        .iter()
        .filter(|t| t.qty < 0)
        .map(|t| t.qty as i64)
        .sum();
    let outstanding = total_owed + total_paid_offset;

    Ok(Json(SupplierBalance {
        total_owed: Amount(total_owed),
        total_paid: Amount(-total_paid_offset),
        outstanding: Amount(outstanding),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::post, Router};
    use http_body_util::BodyExt;
    use ledger::debt::SignedPositionDebt;
    use ledger::{Asset, AssetKind};
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    async fn setup() -> (Router, Arc<AppState>) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::init_pool_with(&pool).await.unwrap();

        // Create warehouses
        sqlx::query("INSERT INTO warehouses (id, name) VALUES (1, 'Warehouse A'), (2, 'Warehouse B')")
            .execute(&pool)
            .await
            .unwrap();

        // Create a product
        sqlx::query("INSERT INTO products (id, name, product_type) VALUES (1, 'Widget', 'product')")
            .execute(&pool)
            .await
            .unwrap();

        let storage = ledger_sqlite::SqliteStorage::from_pool(pool.clone())
            .await
            .unwrap();
        let ledger =
            ledger::Ledger::new(Arc::new(storage)).with_debt_strategy(SignedPositionDebt);
        ledger
            .register_asset(Asset::new("gs", 0, AssetKind::Signed))
            .await
            .unwrap();
        ledger
            .register_asset(Asset::new("product:1", 3, AssetKind::Unsigned))
            .await
            .unwrap();

        let state = Arc::new(AppState { pool, ledger });

        let app = Router::new()
            .route("/inventory/transfer", post(transfer_inventory))
            .with_state(state.clone());

        (app, state)
    }

    async fn seed_stock(state: &AppState, warehouse_id: i64, product_id: i64, qty: f64) {
        let account = format!("@store/{warehouse_id}/product/{product_id}");
        let asset = format!("product:{product_id}");
        let qty_str = format!("{qty:.3}");
        let tx = state
            .ledger
            .transaction(format!("seed-{warehouse_id}-{product_id}"))
            .credit(&account, &asset, &qty_str)
            .build()
            .await
            .unwrap();
        state.ledger.commit(tx).await.unwrap();
    }

    fn transfer_request(from: i64, to: i64, product_id: i64, qty: f64) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/inventory/transfer")
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

        let resp = app
            .oneshot(transfer_request(1, 2, 1, 4.0))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        // Verify balances
        let from_acc = ledger::AccountPath::new("@store/1/product/1").unwrap();
        let to_acc = ledger::AccountPath::new("@store/2/product/1").unwrap();
        let from_bal = state.ledger.balance(&from_acc, "product:1").await.unwrap();
        let to_bal = state.ledger.balance(&to_acc, "product:1").await.unwrap();

        // precision 3 → 6000 = 6.000, 4000 = 4.000
        assert_eq!(from_bal, 6000);
        assert_eq!(to_bal, 4000);
    }

    #[tokio::test]
    async fn transfer_rejects_same_warehouse() {
        let (app, _state) = setup().await;

        let resp = app
            .oneshot(transfer_request(1, 1, 1, 1.0))
            .await
            .unwrap();
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
            .uri("/inventory/transfer")
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

        let resp = app
            .oneshot(transfer_request(1, 2, 1, 0.0))
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn transfer_rejects_negative_quantity() {
        let (app, _state) = setup().await;

        let resp = app
            .oneshot(transfer_request(1, 2, 1, -5.0))
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }
}
