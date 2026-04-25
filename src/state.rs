use sqlx::SqlitePool;

/// Shared application state passed to all route handlers.
pub struct AppState {
    pub pool: SqlitePool,
    pub ledger: ledger::Ledger,
    pub store_id: String,
}
