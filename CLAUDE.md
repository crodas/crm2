# CRM2 — Claude Code Instructions

## Testing

- Every new backend endpoint **must** include unit/integration tests.
- Tests go in `#[cfg(test)] mod tests` at the bottom of the route file.
- Use axum's `tower::ServiceExt::oneshot` pattern with in-memory SQLite for integration tests.
- Test helper `setup()` creates an in-memory DB, runs migrations via `db::init_pool_with`, and returns `(Router, Arc<AppState>)`.
- Add `http-body-util` (dev dep) for reading response bodies in tests.

## Build & Test Commands

- `cargo check` — type-check backend
- `cargo test --all` — run all workspace tests
- `cd frontend && npx tsc --noEmit` — type-check frontend
- `./dev.sh` — run dev server with hot reload
- `./release.sh` — build release binary
