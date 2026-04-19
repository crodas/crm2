# Schema

## Overview

The ledger-sqlite storage uses three tables and two partial indexes, all created by a single migration (`001_ledger.sql`).

## Tables

### ledger_assets

Stores registered asset definitions.

```sql
CREATE TABLE IF NOT EXISTS ledger_assets (
    name      TEXT PRIMARY KEY,
    precision INTEGER NOT NULL,
    kind      TEXT NOT NULL CHECK (kind IN ('signed', 'unsigned'))
);
```

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| `name` | TEXT | PRIMARY KEY | Unique asset identifier (e.g., `"usd"`, `"brush"`) |
| `precision` | INTEGER | NOT NULL | Decimal places (0 for whole units) |
| `kind` | TEXT | NOT NULL, CHECK | `"signed"` or `"unsigned"` |

The CHECK constraint on `kind` prevents invalid values at the database level. Idempotent registration is handled in application code by querying before inserting.

### ledger_transactions

Stores committed transactions as JSON blobs.

```sql
CREATE TABLE IF NOT EXISTS ledger_transactions (
    rowid           INTEGER PRIMARY KEY,
    tx_id           TEXT NOT NULL UNIQUE,
    idempotency_key TEXT NOT NULL UNIQUE,
    data            TEXT NOT NULL
);
```

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| `rowid` | INTEGER | PRIMARY KEY | Auto-incrementing append order |
| `tx_id` | TEXT | NOT NULL, UNIQUE | Deterministic transaction ID (hex SHA-256) |
| `idempotency_key` | TEXT | NOT NULL, UNIQUE | Caller-supplied deduplication key |
| `data` | TEXT | NOT NULL | JSON-serialized `Transaction` struct |

**Design decisions:**
- `rowid` provides append order for `load_transactions()` (ORDER BY rowid)
- `tx_id` UNIQUE prevents duplicate transaction IDs
- `idempotency_key` UNIQUE prevents duplicate submissions
- `data` stores the full `Transaction` as JSON rather than normalizing debits/credits into separate tables. This simplifies the schema and makes `load_transactions()` a single query. The trade-off is that querying individual debits/credits requires deserialization.

### ledger_tokens

Stores spending tokens (the UTXO set).

```sql
CREATE TABLE IF NOT EXISTS ledger_tokens (
    tx_id       TEXT    NOT NULL,
    entry_index INTEGER NOT NULL,
    owner       TEXT    NOT NULL,
    asset_name  TEXT    NOT NULL,
    qty         INTEGER NOT NULL,
    spent_by_tx TEXT,
    PRIMARY KEY (tx_id, entry_index)
);
```

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| `tx_id` | TEXT | NOT NULL, PK | Transaction that created this token |
| `entry_index` | INTEGER | NOT NULL, PK | Position in credits array (0-based) |
| `owner` | TEXT | NOT NULL | Account path that owns this token |
| `asset_name` | TEXT | NOT NULL | Asset name |
| `qty` | INTEGER | NOT NULL | Quantity (scaled by asset precision, stored as i64) |
| `spent_by_tx` | TEXT | NULL | Transaction ID that consumed this token, NULL if unspent |

**Design decisions:**
- Composite primary key `(tx_id, entry_index)` mirrors `EntryRef` in code
- `spent_by_tx` is NULL for unspent tokens. When a token is consumed, this is set to the consuming transaction's `tx_id`
- `qty` is stored as `INTEGER` (SQLite i64), not `TEXT`. This limits quantities to i64 range but enables SQL aggregation (`SUM(qty)`)
- No foreign key to `ledger_transactions` to avoid circular dependency issues during atomic commits

## Indexes

### idx_ledger_tokens_unspent_account

```sql
CREATE INDEX IF NOT EXISTS idx_ledger_tokens_unspent_account
    ON ledger_tokens (owner, asset_name) WHERE spent_by_tx IS NULL;
```

A **partial index** covering only unspent tokens. Optimizes the two most common queries:
- `unspent_by_account()`: exact match on `(owner, asset_name)`
- `unspent_by_prefix()`: range scan on `owner` with exact `asset_name`

As tokens are spent, they fall out of this index, keeping it compact.

### idx_ledger_tokens_unspent_prefix

```sql
CREATE INDEX IF NOT EXISTS idx_ledger_tokens_unspent_prefix
    ON ledger_tokens (asset_name) WHERE spent_by_tx IS NULL;
```

Optimizes queries that filter by asset across multiple owners:
- `unspent_all_by_prefix()`: all unspent tokens under a prefix
- `balances_by_prefix()`: aggregated balances

## Quantity Storage

Quantities are stored as SQLite `INTEGER` (i64) rather than the `i128` used in Rust. This means:

- Maximum quantity: 9,223,372,036,854,775,807 (approximately 92 quadrillion at precision 2)
- Negative quantities are supported (for signed assets with debt)
- SQL `SUM()` aggregation works directly on the column
- Conversion between i128 and i64 happens in the Rust code (`qty as i64` / `qty as i128`)

For the CRM use case (small business accounting), i64 is more than sufficient.

## Database Pragmas

On connection, two SQLite pragmas are set:

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
```

- **WAL (Write-Ahead Logging)**: Allows concurrent reads during writes. Critical for a web application where balance queries should not block transaction commits.
- **Foreign Keys**: Enables foreign key constraint enforcement. While the current schema has no FK constraints, this ensures any future additions are enforced.

## Connection Pool

The `SqlitePool` is configured with a maximum of 5 connections. This limits concurrent database access to prevent lock contention on SQLite's single-writer model.
