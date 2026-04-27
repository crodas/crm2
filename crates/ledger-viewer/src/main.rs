use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use ledger::{Asset, Ledger, Storage};
use ledger_sqlite::SqliteStorage;

#[derive(Serialize)]
struct BalanceResponse {
    account: String,
    asset_name: String,
    raw: i128,
    display: String,
}

#[derive(Serialize)]
struct MovementEntry {
    account: String,
    asset_name: String,
    net: i128,
}

#[derive(Serialize)]
struct DebitEntry {
    account: String,
    asset_name: String,
    raw: i128,
    display: String,
    ref_tx_id: String,
    ref_entry_index: u32,
}

#[derive(Serialize)]
struct CreditEntry {
    account: String,
    asset_name: String,
    raw: i128,
    display: String,
}

#[derive(Serialize)]
struct TransactionResponse {
    tx_id: String,
    idempotency_key: String,
    debits: Vec<DebitEntry>,
    credits: Vec<CreditEntry>,
    movements: Vec<MovementEntry>,
}

#[derive(Serialize)]
struct CreditTokenResponse {
    tx_id: String,
    entry_index: u32,
    owner: String,
    asset_name: String,
    raw: i128,
    display: String,
    spent_by: Option<String>,
}

async fn list_balances(State(ledger): State<Arc<Ledger>>) -> Json<Vec<BalanceResponse>> {
    let entries = ledger.balances_by_prefix("").await.unwrap_or_default();
    let balances = entries
        .into_iter()
        .map(|e| BalanceResponse {
            account: e.account,
            asset_name: e.amount.asset_name().to_string(),
            raw: e.amount.raw(),
            display: e.amount.to_decimal_string(),
        })
        .collect();
    Json(balances)
}

async fn list_transactions(State(ledger): State<Arc<Ledger>>) -> Json<Vec<TransactionResponse>> {
    let txs = ledger.transactions().await.unwrap_or_default();
    let responses: Vec<TransactionResponse> = txs
        .into_iter()
        .map(|tx| {
            let movements = tx
                .net_movements()
                .into_iter()
                .map(|m| MovementEntry {
                    account: m.account,
                    asset_name: m.asset_name,
                    net: m.net_raw,
                })
                .collect();
            TransactionResponse {
                tx_id: tx.tx_id,
                idempotency_key: tx.idempotency_key,
                debits: tx
                    .debits
                    .into_iter()
                    .map(|d| DebitEntry {
                        account: d.from,
                        asset_name: d.amount.asset_name().to_string(),
                        raw: d.amount.raw(),
                        display: d.amount.to_decimal_string(),
                        ref_tx_id: d.tx_id,
                        ref_entry_index: d.entry_index,
                    })
                    .collect(),
                credits: tx
                    .credits
                    .into_iter()
                    .map(|c| CreditEntry {
                        account: c.to,
                        asset_name: c.amount.asset_name().to_string(),
                        raw: c.amount.raw(),
                        display: c.amount.to_decimal_string(),
                    })
                    .collect(),
                movements,
            }
        })
        .collect();
    Json(responses)
}

async fn list_credit_tokens(State(ledger): State<Arc<Ledger>>) -> Json<Vec<CreditTokenResponse>> {
    let txs = ledger.transactions().await.unwrap_or_default();
    let mut credit_tokens: HashMap<(String, u32), CreditTokenResponse> = HashMap::new();

    for tx in &txs {
        for (i, c) in tx.credits.iter().enumerate() {
            let idx = i as u32;
            credit_tokens.insert(
                (tx.tx_id.clone(), idx),
                CreditTokenResponse {
                    tx_id: tx.tx_id.clone(),
                    entry_index: idx,
                    owner: c.to.clone(),
                    asset_name: c.amount.asset_name().to_string(),
                    raw: c.amount.raw(),
                    display: c.amount.to_decimal_string(),
                    spent_by: None,
                },
            );
        }
        for d in &tx.debits {
            if let Some(credit) = credit_tokens.get_mut(&(d.tx_id.clone(), d.entry_index)) {
                credit.spent_by = Some(tx.tx_id.clone());
            }
        }
    }

    let mut result: Vec<CreditTokenResponse> = credit_tokens.into_values().collect();
    result.sort_by(|a, b| {
        a.tx_id
            .cmp(&b.tx_id)
            .then(a.entry_index.cmp(&b.entry_index))
    });
    Json(result)
}

async fn list_assets(State(ledger): State<Arc<Ledger>>) -> Json<HashMap<String, Asset>> {
    Json((*ledger.assets()).clone())
}

async fn viewer() -> Html<&'static str> {
    Html(include_str!("viewer.html"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sqlite:crm2.db?mode=ro".to_string());

    eprintln!("Connecting to {db_url}");
    let storage = SqliteStorage::connect(&db_url).await?;
    let storage: Arc<dyn Storage> = Arc::new(storage);

    let assets = storage.load_assets().await?;
    let ledger = Ledger::new(Arc::clone(&storage));
    for asset in assets.values() {
        ledger.register_asset(asset.clone()).await?;
    }

    let ledger = Arc::new(ledger);

    let app = Router::new()
        .route("/", get(viewer))
        .route("/api/balances", get(list_balances))
        .route("/api/transactions", get(list_transactions))
        .route("/api/credit-tokens", get(list_credit_tokens))
        .route("/api/assets", get(list_assets))
        .with_state(ledger);

    let addr = "127.0.0.1:3001";
    eprintln!("Ledger viewer at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
