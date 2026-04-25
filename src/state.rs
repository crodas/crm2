use crate::storage;

/// Shared application state passed to all route handlers.
pub struct AppState {
    pub db: storage::Db,
}
