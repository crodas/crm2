# Storage Contract

## Overview

`SqliteStorage` implements the `ledger-core::Storage` trait, providing durable, ACID-compliant persistence for the UTXO ledger.

## Trait Requirements

The `Storage` trait requires:
- `Send + Sync` -- safe to share across async tasks
- `Debug` -- printable for diagnostics
- Granular write methods, each wrapped in its own database transaction

`SqliteStorage` meets all three:
- `SqlitePool` is `Send + Sync`
- Custom `Debug` impl uses `finish_non_exhaustive()` to avoid printing pool internals
- Every write method uses `pool.begin()` / `tx.commit()` for atomicity

## Construction

### New Pool

```rust
let storage = SqliteStorage::connect("sqlite:ledger.db?mode=rwc").await?;
```

Creates a new connection pool with:
- Max 5 connections
- WAL journal mode enabled
- Foreign keys enabled
- Migrations run automatically

### Shared Pool

```rust
let storage = SqliteStorage::from_pool(existing_pool).await?;
```

Reuses an existing `SqlitePool`. This is the typical pattern in the CRM app, where the ledger tables coexist with application tables in the same database. Migrations still run on the shared pool.

## Atomicity

Every write method wraps its operations in a `sqlx::Transaction`:

- **`register_asset`** — SELECT + INSERT in one transaction
- **`mark_spent`** — batch UPDATE with CAS guard + fallback reads in one transaction
- **`unmark_spent`** — batch UPDATE in one transaction
- **`insert_tokens`** — all INSERTs in one transaction
- **`remove_tokens`** — all DELETEs in one transaction
- **`insert_tx`** — single INSERT in one transaction
- **`remove_tx`** — single DELETE in one transaction

The saga layer composes these methods into a three-step commit (mark spent → create tokens → insert transaction) with automatic compensation on failure.

### CAS Guard on mark_spent

`mark_spent` uses a compare-and-swap pattern at the SQL level:

```sql
UPDATE ledger_tokens SET spent_by_tx = ?
WHERE (tx_id, entry_index) IN (VALUES (?,?), (?,?), ...)
AND spent_by_tx IS NULL
```

If `rows_affected != refs.len()`, at least one token was already spent. A follow-up query identifies the culprit and returns `LedgerError::AlreadySpent`.

### tx_to_revert Guard on unmark_spent

`unmark_spent` only reverts tokens spent by the specified transaction:

```sql
UPDATE ledger_tokens SET spent_by_tx = NULL
WHERE (tx_id, entry_index) IN (VALUES (?,?), (?,?), ...)
AND spent_by_tx = ?
```

This prevents accidentally unmarking tokens spent by a different (legitimate) transaction during compensation.

## Type Conversions

### Quantity: i128 <-> i64

Quantities are stored as SQLite `INTEGER` (i64). The Rust code uses `qty as i64` for storage and `qty as i128` for retrieval. This is safe for the expected value range (small business accounting).

### TokenStatus

- `spent_by_tx IS NULL` → `TokenStatus::Unspent`
- `spent_by_tx = "some_tx_id"` → `TokenStatus::Spent(0)`

For `get_token()`, the spend status is determined by the presence of `spent_by_tx`. For unspent query methods, status is always `Unspent` (the query filters out spent tokens).

### Transaction Serialization

Transactions are serialized as JSON via `serde_json::to_string()` and stored in the `data` TEXT column. This avoids normalizing debits and credits into separate tables, simplifying the schema at the cost of not being able to query individual entries via SQL.

## Conformance Testing

The crate uses `ledger-core`'s `storage_tests!` macro to run the full conformance suite:

```rust
#[cfg(test)]
mod tests {
    use ledger_core::storage_tests;
    storage_tests!(async { SqliteStorage::connect("sqlite::memory:").await.expect("connect") });
}
```

This generates 40+ tests covering all Storage trait methods, ensuring SqliteStorage is fully compatible with the Ledger engine.

## Error Mapping

All `sqlx::Error` values are converted to `LedgerError::Storage(String)`:

```rust
fn db_err(e: sqlx::Error) -> LedgerError {
    LedgerError::Storage(e.to_string())
}
```

This keeps the storage error opaque to the ledger -- it doesn't need to know whether the backend is SQLite, Postgres, or something else.
