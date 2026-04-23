use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Json;
use axum::Router;
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
}

/// Compute balances from transactions (avoids prefix query limitations).
fn balances_from_transactions(
    txs: &[ledger::Transaction],
) -> Vec<BalanceResponse> {
    let mut map: HashMap<(String, String), (i128, Option<Asset>)> = HashMap::new();

    for tx in txs {
        for d in &tx.debits {
            let key = (d.from.clone(), d.amount.asset_name().to_string());
            let entry = map.entry(key).or_insert((0, None));
            entry.0 -= d.amount.raw();
            if entry.1.is_none() {
                entry.1 = Some(d.amount.asset().clone());
            }
        }
        for c in &tx.credits {
            let key = (c.to.clone(), c.amount.asset_name().to_string());
            let entry = map.entry(key).or_insert((0, None));
            entry.0 += c.amount.raw();
            if entry.1.is_none() {
                entry.1 = Some(c.amount.asset().clone());
            }
        }
    }

    let mut balances: Vec<BalanceResponse> = map
        .into_iter()
        .filter(|(_, (raw, _))| *raw != 0)
        .map(|((account, asset_name), (raw, asset))| {
            let display = match asset {
                Some(a) => a.from_cents(raw),
                None => raw.to_string(),
            };
            BalanceResponse {
                account,
                asset_name,
                raw,
                display,
            }
        })
        .collect();

    balances.sort_by(|a, b| a.account.cmp(&b.account).then(a.asset_name.cmp(&b.asset_name)));
    balances
}

async fn list_balances(State(ledger): State<Arc<Ledger>>) -> Json<Vec<BalanceResponse>> {
    let txs = ledger.transactions().await.unwrap_or_default();
    Json(balances_from_transactions(&txs))
}

async fn list_transactions(State(ledger): State<Arc<Ledger>>) -> Json<Vec<TransactionResponse>> {
    let txs = ledger.transactions().await.unwrap_or_default();
    let responses: Vec<TransactionResponse> = txs
        .into_iter()
        .map(|tx| TransactionResponse {
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
        })
        .collect();
    Json(responses)
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

    // Load existing assets into the ledger cache.
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
        .route("/api/assets", get(list_assets))
        .with_state(ledger);

    let addr = "127.0.0.1:3001";
    eprintln!("Ledger viewer at http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
