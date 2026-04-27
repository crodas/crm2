# Queries

## Overview

All queries use parameterized bindings (`?` placeholders) for SQL injection safety. Errors from SQLx are converted to `LedgerError::Storage(String)` via the `db_err()` helper. Write methods wrap their operations in explicit `sqlx::Transaction`s.

## Asset Queries

### Check Existing Asset

```sql
SELECT precision FROM ledger_assets WHERE name = ?
```

Used during `register_asset()` to check for conflicts before inserting. If the asset exists with matching precision, the operation is a no-op. If it exists with different precision, returns `AssetConflict`.

### Insert Asset

```sql
INSERT INTO ledger_assets (name, precision, kind) VALUES (?, ?, 'signed')
```

Only executed after confirming the asset doesn't already exist. Both operations run in a single transaction.

### Load All Assets

```sql
SELECT name, precision FROM ledger_assets
```

Returns all registered assets.

## Idempotency Check

```sql
SELECT 1 FROM ledger_transactions WHERE idempotency_key = ?
```

Returns a row if the key exists. Uses the UNIQUE index on `idempotency_key` for O(log n) lookup.

## Credit Token Queries

All credit token queries join `ledger_credit_tokens` with `ledger_assets` to retrieve precision:

```sql
SELECT t.tx_id, t.entry_index, t.owner, t.asset_name, t.qty, t.spent_by_tx,
       a.precision
FROM ledger_credit_tokens t
JOIN ledger_assets a ON a.name = t.asset_name
```

### Get Credit Token by CreditEntryRef

```sql
... WHERE t.tx_id = ? AND t.entry_index = ?
```

Fetches a single token by its composite primary key. Returns `None` if not found. The `spent_by_tx` column determines `CreditTokenStatus`:
- NULL → `CreditTokenStatus::Unspent`
- non-NULL → `CreditTokenStatus::Spent(0)`

### Unspent Credit Tokens by Exact Account

```sql
... WHERE t.owner = ? AND t.asset_name = ? AND t.spent_by_tx IS NULL
```

Uses the `idx_ledger_credit_tokens_unspent_account` partial index for efficient lookup. Returns only unspent credit tokens for the exact account (no descendants).

### Unspent Credit Tokens by Prefix

```sql
... WHERE (t.owner = ? OR t.owner LIKE ?)
      AND t.asset_name = ?
      AND t.spent_by_tx IS NULL
```

The two conditions handle:
- `owner = ?` → exact match (e.g., `store`)
- `owner LIKE ?` → descendant match (e.g., `store/%`)

The LIKE pattern is constructed as `{prefix}/%` in Rust code.

### Aggregated Balances by Prefix

```sql
SELECT t.owner, t.asset_name, SUM(t.qty) as balance, a.precision
FROM ledger_credit_tokens t
JOIN ledger_assets a ON a.name = t.asset_name
WHERE (t.owner = ? OR t.owner LIKE ?)
  AND t.spent_by_tx IS NULL
GROUP BY t.owner, t.asset_name
HAVING SUM(t.qty) != 0
ORDER BY t.owner, t.asset_name
```

Groups unspent credit tokens by (account, asset) and sums their quantities. The `HAVING` clause excludes zero-balance groups. Results are sorted for deterministic output.

## Write Operations

### Mark Spent (Batch with CAS)

```sql
UPDATE ledger_credit_tokens SET spent_by_tx = ?
WHERE (tx_id, entry_index) IN (VALUES (?,?), (?,?), ...)
AND spent_by_tx IS NULL
```

Single batched UPDATE for all refs. The `AND spent_by_tx IS NULL` clause acts as a compare-and-swap guard — if any token was already spent, `rows_affected` will be less than expected and the method identifies the culprit via a follow-up query, returning `AlreadySpent`.

### Unmark Spent (Batch with tx_to_revert Guard)

```sql
UPDATE ledger_credit_tokens SET spent_by_tx = NULL
WHERE (tx_id, entry_index) IN (VALUES (?,?), (?,?), ...)
AND spent_by_tx = ?
```

Only reverts tokens whose `spent_by_tx` matches the specified `tx_to_revert`. Tokens spent by other transactions are left untouched.

### Insert Credit Tokens

```sql
INSERT INTO ledger_credit_tokens (tx_id, entry_index, owner, asset_name, qty)
VALUES (?, ?, ?, ?, ?)
```

One row per credit in the transaction. `spent_by_tx` defaults to NULL (unspent). All inserts run in a single transaction.

### Remove Credit Tokens

```sql
DELETE FROM ledger_credit_tokens WHERE tx_id = ? AND entry_index = ?
```

One delete per ref. All deletes run in a single transaction.

### Insert Transaction

```sql
INSERT INTO ledger_transactions (tx_id, idempotency_key, data) VALUES (?, ?, ?)
```

The `data` column receives a `serde_json::to_string()` serialization of the full `Transaction` struct.

### Remove Transaction

```sql
DELETE FROM ledger_transactions WHERE tx_id = ?
```

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

## Helper: rows_to_credit_tokens

The `rows_to_credit_tokens()` function converts SQLx result rows into `Vec<CreditToken>`. It:

1. Extracts `tx_id`, `entry_index`, `owner`, and `qty` from each row
2. Reconstructs the `Asset` from `asset_name` and `precision`
3. Builds an `Amount` via `asset.amount_unchecked(qty)`
4. Sets status to `CreditTokenStatus::Unspent` (only used by unspent-query methods)
