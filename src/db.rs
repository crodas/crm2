use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

pub async fn init_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await?;

    run_migrations(&pool).await?;
    Ok(pool)
}

/// Run migrations on an already-connected pool (useful for tests with in-memory DBs).
pub async fn init_pool_with(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    run_migrations(pool).await
}

async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;

    let migrations: Vec<(&str, &str)> = vec![
        ("001_initial", include_str!("../migrations/001_initial.sql")),
        (
            "002_products_inventory",
            include_str!("../migrations/002_products_inventory.sql"),
        ),
        (
            "003_services",
            include_str!("../migrations/003_services.sql"),
        ),
        (
            "004_sort_order",
            include_str!("../migrations/004_sort_order.sql"),
        ),
        (
            "005_services_catalog",
            include_str!("../migrations/005_services_catalog.sql"),
        ),
        (
            "006_version_chain",
            include_str!("../migrations/006_version_chain.sql"),
        ),
        (
            "007_version_chain_quotes_bookings",
            include_str!("../migrations/007_version_chain_quotes_bookings.sql"),
        ),
        (
            "008_merge_work_orders_into_bookings",
            include_str!("../migrations/008_merge_work_orders_into_bookings.sql"),
        ),
        (
            "009_supplier_ledger",
            include_str!("../migrations/009_supplier_ledger.sql"),
        ),
        (
            "010_drop_old_utxo",
            include_str!("../migrations/010_drop_old_utxo.sql"),
        ),
        (
            "011_sale_payments",
            include_str!("../migrations/011_sale_payments.sql"),
        ),
    ];

    for (name, sql) in migrations {
        let applied: bool =
            sqlx::query_scalar("SELECT COUNT(*) > 0 FROM _migrations WHERE name = ?")
                .bind(name)
                .fetch_one(pool)
                .await?;

        if !applied {
            tracing::info!("Applying migration: {name}");
            for statement in sql.split(';') {
                let stmt = statement.trim();
                if stmt.is_empty() {
                    continue;
                }
                sqlx::query(stmt).execute(pool).await?;
            }
            sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
                .bind(name)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}
