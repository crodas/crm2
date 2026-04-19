# Queries

## Overview

All queries use parameterized bindings (`?` placeholders) for SQL injection safety. Errors from SQLx are converted to `LedgerError::Storage(String)` via the `db_err()` helper.

## Asset Queries

### Check Existing Asset

```sql
SELECT precision, kind FROM ledger_assets WHERE name = ?
```

Used during `register_asset()` to check for conflicts before inserting. If the asset exists with matching precision and kind, the operation is a no-op. If it exists with different values, returns `AssetConflict`.

### Insert Asset

```sql
INSERT INTO ledger_assets (name, precision, kind) VALUES (?, ?, ?)
```

Only executed after confirming the asset doesn't already exist.

### Load All Assets

```sql
SELECT name, precision, kind FROM ledger_assets
```

Returns all registered assets. Called during `Ledger::new()` to populate the in-memory cache.

## Idempotency Check

```sql
SELECT 1 FROM ledger_transactions WHERE idempotency_key = ?
```

Returns a row if the key exists. Uses the UNIQUE index on `idempotency_key` for O(log n) lookup.

## Token Queries

### Get Token by EntryRef

```sql
SELECT tx_id, entry_index, owner, asset_name, qty, spent_by_tx
FROM ledger_tokens WHERE tx_id = ? AND entry_index = ?
```

Fetches a single token by its composite primary key. Returns `None` if not found. The `spent_by_tx` column determines `TokenStatus`:
- NULL → `TokenStatus::Unspent`
- non-NULL → `TokenStatus::Spent(tx_index)` (where tx_index is looked up)

### Unspent Tokens by Exact Account

```sql
SELECT tx_id, entry_index, owner, asset_name, qty
FROM ledger_tokens
WHERE owner = ? AND asset_name = ? AND spent_by_tx IS NULL
```

Uses the `idx_ledger_tokens_unspent_account` partial index for efficient lookup. Returns only unspent tokens for the exact account (no descendants).

### Unspent Tokens by Prefix

```sql
SELECT tx_id, entry_index, owner, asset_name, qty
FROM ledger_tokens
WHERE (owner = ? OR owner LIKE ?)
  AND asset_name = ?
  AND spent_by_tx IS NULL
```

The two conditions handle:
- `owner = ?` → exact match (e.g., `@store`)
- `owner LIKE ?` → descendant match (e.g., `@store/%`)

The LIKE pattern is constructed as `{prefix}/%` in Rust code.

### Unspent Tokens by Prefix (All Assets)

```sql
SELECT tx_id, entry_index, owner, asset_name, qty
FROM ledger_tokens
WHERE (owner = ? OR owner LIKE ?)
  AND spent_by_tx IS NULL
```

Same as above but without the `asset_name` filter. Returns all unspent tokens across all assets under the prefix.

### Aggregated Balances by Prefix

```sql
SELECT owner, asset_name, SUM(qty) as balance
FROM ledger_tokens
WHERE (owner = ? OR owner LIKE ?)
  AND spent_by_tx IS NULL
GROUP BY owner, asset_name
HAVING SUM(qty) != 0
ORDER BY owner, asset_name
```

Groups unspent tokens by (account, asset) and sums their quantities. The `HAVING` clause excludes zero-balance groups (which can occur when positive and negative tokens cancel out). Results are sorted for deterministic output.

## Transaction Persistence

### Insert Transaction

```sql
INSERT INTO ledger_transactions (tx_id, idempotency_key, data) VALUES (?, ?, ?)
```

The `data` column receives a `serde_json::to_string()` serialization of the full `Transaction` struct.

### Insert Token

```sql
INSERT INTO ledger_tokens (tx_id, entry_index, owner, asset_name, qty) VALUES (?, ?, ?, ?, ?)
```

One row per credit in the transaction. `spent_by_tx` defaults to NULL (unspent).

### Mark Token as Spent

```sql
UPDATE ledger_tokens SET spent_by_tx = ? WHERE tx_id = ? AND entry_index = ?
```

Sets `spent_by_tx` to the consuming transaction's ID. One update per debit in the transaction.

### Atomic Commit

All three operations (insert transaction, insert tokens, mark spent) execute within a single SQLx transaction:

```rust
let mut db_tx = pool.begin().await?;
// INSERT INTO ledger_transactions ...
// INSERT INTO ledger_tokens ... (for each new token)
// UPDATE ledger_tokens SET spent_by_tx ... (for each debit)
db_tx.commit().await?;
```

If any step fails, the database transaction rolls back and no changes are visible.

## Read Queries

### Load All Transactions

```sql
SELECT data FROM ledger_transactions ORDER BY rowid
```

Returns JSON blobs in append order. Each blob is deserialized into a `Transaction` struct via `serde_json::from_str()`.

### Count Transactions

```sql
SELECT COUNT(*) as cnt FROM ledger_transactions
```

Returns the total number of committed transactions.

## Helper: rows_to_tokens

The `rows_to_tokens()` function converts SQLx result rows into `Vec<SpendingToken>`. It:

1. Extracts `tx_id`, `entry_index`, `owner`, `asset_name`, `qty` from each row
2. Constructs an `AccountPath` from the owner string (validating it)
3. Sets status to `TokenStatus::Unspent` (only used by unspent-query methods)
4. Returns `LedgerError::InvalidAccount` if the owner fails validation
