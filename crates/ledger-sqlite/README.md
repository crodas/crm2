# ledger-sqlite

SQLite-backed storage implementation for `ledger-core`. Provides persistent, ACID-compliant storage via SQLx with WAL mode and optimized partial indexes.

## Overview

`ledger-sqlite` implements the `ledger-core::Storage` trait using SQLite as the persistence backend. It manages three tables (`ledger_assets`, `ledger_transactions`, `ledger_tokens`) with atomic commits, partial indexes for unspent token queries, and JSON serialization for transaction data.

## Architecture

```
ledger-sqlite/
  src/
    lib.rs              # SqliteStorage struct and Storage trait impl
  migrations/
    001_ledger.sql      # Schema: tables and indexes
```

## Usage

```rust
use ledger_sqlite::SqliteStorage;
use ledger_core::Ledger;
use std::sync::Arc;

// Create with new connection pool
let storage = Arc::new(SqliteStorage::connect("sqlite:ledger.db?mode=rwc").await?);
let ledger = Ledger::new(storage);

// Or share an existing pool (e.g., with the app database)
let storage = Arc::new(SqliteStorage::from_pool(existing_pool).await?);
```

## Documentation

See [`doc/`](doc/) for detailed technical documentation:

- [Schema](doc/schema.md) -- tables, indexes, constraints, and design rationale
- [Queries](doc/queries.md) -- every SQL query with explanation
- [Storage Contract](doc/storage-contract.md) -- how the Storage trait is fulfilled
