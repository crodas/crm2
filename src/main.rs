mod amount;
mod db;
mod error;
mod models;
mod routes;
mod seed;
mod state;
mod storage;
mod version;

use std::path::Path;
use std::sync::Arc;

use axum::{
    routing::{get, patch, post, put},
    Router,
};
use ledger::debt::SignedPositionDebt;
use ledger::Asset;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

fn build_frontend(dev: bool) -> Result<(), Box<dyn std::error::Error>> {
    let frontend_dir = Path::new("frontend");
    if !frontend_dir.join("node_modules").exists() {
        tracing::info!("Installing frontend dependencies...");
        let status = std::process::Command::new("npm")
            .arg("install")
            .current_dir(frontend_dir)
            .status()?;
        if !status.success() {
            return Err("npm install failed".into());
        }
    }

    // Always do an initial build so frontend/dist/ exists immediately
    tracing::info!("Building frontend...");
    let status = std::process::Command::new("npx")
        .args(["vite", "build"])
        .current_dir(frontend_dir)
        .status()?;
    if !status.success() {
        return Err("vite build failed".into());
    }

    if dev {
        // In dev: keep rebuilding on file changes
        tracing::info!("Watching frontend for changes...");
        std::thread::spawn(|| {
            let _ = std::process::Command::new("npx")
                .args(["vite", "build", "--watch"])
                .current_dir("frontend")
                .status();
        });
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let is_release = !cfg!(debug_assertions);

    // Build frontend
    if Path::new("frontend/package.json").exists() {
        build_frontend(!is_release)?;
    }

    // Initialize CRM database
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:crm2.db?mode=rwc".into());
    let pool = db::init_pool(&database_url).await?;

    // Initialize ledger with its own separate database
    let ledger_url =
        std::env::var("LEDGER_URL").unwrap_or_else(|_| "sqlite:ledger.db?mode=rwc".into());
    let ledger_storage = ledger_sqlite::SqliteStorage::connect(&ledger_url).await?;
    let aliases = ledger_storage.load_aliases().await?;
    let ledger = ledger::Ledger::new(Arc::new(ledger_storage))
        .with_aliases(aliases)
        .with_debt_strategy(
            SignedPositionDebt::new("customer/{from}", "warehouse/{to}/receivables/{from}"),
        );

    let store_id = std::env::var("STORE_ID").unwrap_or_else(|_| "1".into());
    let db = storage::Db::new(pool, ledger, store_id);

    // Register monetary asset
    db.register_asset(Asset::new("gs", 0)).await?;

    // Register one asset per product (precision 3 = thousandths for fractional quantities)
    let product_ids = db.product_ids().await?;
    for id in &product_ids {
        db.register_asset(Asset::new(format!("product:{id}"), 3))
            .await?;
    }
    tracing::info!(
        "Ledger initialized with {} product assets",
        product_ids.len()
    );

    let state = Arc::new(AppState { db });

    // Seed dev data (only in debug mode)
    if !is_release {
        seed::seed_dev_data(&state)
            .await
            .map_err(|e| e.to_string())?;
    }

    // API routes
    let api = Router::new()
        // Config
        .route(
            "/config",
            get(routes::config::get_config).put(routes::config::update_config),
        )
        // Customer types
        .route(
            "/customer-types",
            get(routes::customers::list_customer_types)
                .post(routes::customers::create_customer_type),
        )
        .route(
            "/customer-types/{id}",
            put(routes::customers::update_customer_type),
        )
        .route(
            "/customer-types/reorder",
            put(routes::customers::reorder_customer_types),
        )
        // Customer groups
        .route(
            "/customer-groups",
            get(routes::customer_groups::list_groups).post(routes::customer_groups::create_group),
        )
        .route(
            "/customer-groups/{id}",
            put(routes::customer_groups::update_group),
        )
        // Customers
        .route(
            "/customers",
            get(routes::customers::list_customers).post(routes::customers::create_customer),
        )
        .route(
            "/customers/{id}",
            get(routes::customers::get_customer).put(routes::customers::update_customer),
        )
        .route(
            "/customers/{id}/timeline",
            get(routes::customers::customer_timeline),
        )
        .route(
            "/customers/{id}/balance",
            get(routes::payments::customer_balance),
        )
        .route("/receivables", get(routes::payments::total_receivables))
        // Products
        .route(
            "/products",
            get(routes::products::list_products).post(routes::products::create_product),
        )
        .route(
            "/products/{id}",
            get(routes::products::get_product).put(routes::products::update_product),
        )
        // Warehouses
        .route(
            "/warehouses",
            get(routes::warehouses::list_warehouses).post(routes::warehouses::create_warehouse),
        )
        .route(
            "/warehouses/{id}",
            put(routes::warehouses::update_warehouse),
        )
        .route(
            "/warehouses/reorder",
            put(routes::warehouses::reorder_warehouses),
        )
        // Inventory
        .route(
            "/inventory/receive",
            post(routes::inventory::receive_inventory),
        )
        .route("/inventory/stock", get(routes::inventory::get_stock))
        .route("/inventory/receipts", get(routes::inventory::list_receipts))
        .route(
            "/inventory/receipts/{id}",
            get(routes::inventory::get_receipt),
        )
        .route(
            "/inventory/receipts/{id}/payments",
            post(routes::inventory::record_supplier_payment),
        )
        .route(
            "/supplier-balance",
            get(routes::inventory::supplier_balance),
        )
        .route("/inventory/prices", get(routes::inventory::latest_prices))
        .route(
            "/inventory/transfers",
            get(routes::inventory::list_transfers).post(routes::inventory::transfer_inventory),
        )
        // Sales
        .route(
            "/sales",
            get(routes::sales::list_sales).post(routes::sales::create_sale),
        )
        .route("/sales/{id}", get(routes::sales::get_sale))
        .route(
            "/sales/{id}/payments",
            post(routes::sales::record_sale_payment),
        )
        // Teams
        .route(
            "/teams",
            get(routes::teams::list_teams).post(routes::teams::create_team),
        )
        .route("/teams/{id}", put(routes::teams::update_team))
        .route(
            "/teams/{id}/members",
            get(routes::teams::list_members).post(routes::teams::add_member),
        )
        // Quotes
        .route(
            "/quotes",
            get(routes::quotes::list_quotes).post(routes::quotes::create_quote),
        )
        .route(
            "/quotes/{id}",
            get(routes::quotes::get_quote).put(routes::quotes::update_quote),
        )
        .route(
            "/quotes/{id}/status",
            patch(routes::quotes::update_quote_status),
        )
        .route(
            "/quotes/{id}/payments",
            post(routes::payments::record_payment),
        )
        // Debts
        .route("/debts", post(routes::quotes::create_debt))
        // Bookings
        .route(
            "/bookings",
            get(routes::bookings::list_bookings).post(routes::bookings::create_booking),
        )
        .route(
            "/bookings/{id}",
            get(routes::bookings::get_booking).put(routes::bookings::update_booking),
        )
        .route(
            "/bookings/{id}/quotes/{quote_id}",
            post(routes::bookings::link_quote),
        )
        .route(
            "/bookings/{id}/quotes/{quote_id}/unlink",
            post(routes::bookings::unlink_quote),
        )
        // Calendar
        .route("/calendar", get(routes::calendar::get_calendar));

    let spa_fallback = ServeFile::new("frontend/dist/index.html");
    let serve_dir = ServeDir::new("frontend/dist").fallback(spa_fallback);

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(serve_dir)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:3000";
    tracing::info!("Server running on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
