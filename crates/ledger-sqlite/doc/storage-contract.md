# Storage Contract

## Overview

`SqliteStorage` implements the `ledger-core::Storage` trait, providing durable, ACID-compliant persistence for the UTXO ledger.

## Trait Requirements

The `Storage` trait requires:
- `Send + Sync` -- safe to share across async tasks
- `Debug` -- printable for diagnostics
- Atomic `commit_tx` -- all-or-nothing transaction persistence

`SqliteStorage` meets all three:
- `SqlitePool` is `Send + Sync`
- Custom `Debug` impl uses `finish_non_exhaustive()` to avoid printing pool internals
- `commit_tx` uses SQLx transactions for atomicity

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

## Atomicity Guarantee

The `commit_tx` method wraps three operations in a single database transaction:

```
BEGIN
  1. INSERT INTO ledger_transactions (tx record)
  2. INSERT INTO ledger_tokens (new tokens, one per credit)
  3. UPDATE ledger_tokens SET spent_by_tx = ? (mark consumed tokens)
COMMIT
```

If any step fails (constraint violation, I/O error), SQLx rolls back the entire transaction. This guarantees:
- A committed transaction's tokens are always visible
- Spent markers are always set when the spending transaction exists
- No partial state (e.g., transaction inserted but tokens missing)

## Type Conversions

### AssetKind <-> String

| Rust | SQL |
|------|-----|
| `AssetKind::Signed` | `"signed"` |
| `AssetKind::Unsigned` | `"unsigned"` |

Conversion functions: `kind_to_str()` and `str_to_kind()`.

### Quantity: i128 <-> i64

Quantities are stored as SQLite `INTEGER` (i64). The Rust code uses `qty as i64` for storage and `qty as i128` for retrieval. This is safe for the expected value range (small business accounting).

### TokenStatus

- `spent_by_tx IS NULL` → `TokenStatus::Unspent`
- `spent_by_tx = "some_tx_id"` → `TokenStatus::Spent(tx_index)`

For `get_token()`, the spend status includes the transaction index. For unspent query methods, status is always `Unspent` (the query filters out spent tokens).

### Transaction Serialization

Transactions are serialized as JSON via `serde_json::to_string()` and stored in the `data` TEXT column. This avoids normalizing debits and credits into separate tables, simplifying the schema at the cost of not being able to query individual entries via SQL.

## Conformance Testing

The crate uses `ledger-core`'s `storage_tests!` macro to run the full conformance suite:

```rust
#[cfg(test)]
mod tests {
    use ledger_core::storage::test_support::storage_tests;
    storage_tests!(async { SqliteStorage::connect(":memory:").await });
}
```

This generates 45+ tests covering all Storage trait methods, ensuring SqliteStorage is fully compatible with the Ledger engine.

## Error Mapping

All `sqlx::Error` values are converted to `LedgerError::Storage(String)`:

```rust
fn db_err(e: sqlx::Error) -> LedgerError {
    LedgerError::Storage(e.to_string())
}
```

This keeps the storage error opaque to the ledger -- it doesn't need to know whether the backend is SQLite, Postgres, or something else.
