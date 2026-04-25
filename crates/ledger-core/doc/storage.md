# Storage

## Storage Trait

The `Storage` trait defines the persistence contract for the ledger. Any backend that implements this trait can be used with `Ledger`.

```rust
#[async_trait]
pub trait Storage: Send + Sync + Debug {
    // Asset management
    async fn register_asset(&self, asset: &Asset) -> Result<(), LedgerError>;
    async fn load_assets(&self) -> Result<HashMap<String, Asset>, LedgerError>;

    // Idempotency
    async fn has_idempotency_key(&self, key: &str) -> Result<bool, LedgerError>;

    // Token queries
    async fn get_token(&self, eref: &EntryRef) -> Result<Option<SpendingToken>, LedgerError>;
    async fn unspent_by_account(&self, account: &str, requested_amount: Option<&Amount>)
        -> Result<Vec<SpendingToken>, LedgerError>;
    async fn unspent_by_prefix(&self, prefix: &str, requested_amount: Option<&Amount>)
        -> Result<Vec<SpendingToken>, LedgerError>;
    async fn balances_by_prefix(&self, prefix: &str)
        -> Result<Vec<BalanceEntry>, LedgerError>;

    // Transaction persistence
    async fn commit_tx(
        &self,
        tx: &Transaction,
        new_tokens: &[SpendingToken],
        spent_refs: &[EntryRef],
    ) -> Result<(), LedgerError>;
    async fn load_transactions(&self) -> Result<Vec<Transaction>, LedgerError>;
    async fn tx_count(&self) -> Result<usize, LedgerError>;
}
```

### Method Contracts

#### `register_asset`

- Must persist the asset definition durably
- **Idempotent**: registering the same asset (name, precision, kind) twice is a silent no-op
- Must return `AssetConflict` if the name exists with different precision or kind

#### `has_idempotency_key`

- Returns `true` if a transaction with this key has been committed
- Must be consistent with `commit_tx` -- if `commit_tx` succeeds, all subsequent calls must return `true` for that key

#### `get_token`

- Returns `Some(SpendingToken)` if a token exists at the given `EntryRef`
- Returns `None` if no token exists at that reference
- Must reflect the current spend status (Unspent or Spent)

#### `unspent_by_account`

- `Some(amount)` — returns all tokens with `status == Unspent` for the **exact** account and the amount's asset
- `None` — returns all unspent tokens for the account across all assets
- Must **not** include descendant accounts (e.g., `store` does not include `store/cash`)

#### `unspent_by_prefix`

- `Some(amount)` — returns all tokens with `status == Unspent` where the owner matches the prefix **or** is a descendant, filtered by the amount's asset
- `None` — same but across all assets
- Prefix matching: owner == prefix OR owner starts with `{prefix}/`

#### `balances_by_prefix`

- Returns aggregated sums grouped by (account, asset)
- Only includes groups with non-zero balances
- Sorted by account, then asset name

#### `commit_tx`

**Critical atomicity requirement**: This method must be all-or-nothing. It performs three operations that must all succeed or all fail:

1. Insert the transaction record (with tx_id, idempotency_key, and full data)
2. Insert all new spending tokens (one per credit in the transaction)
3. Mark all consumed tokens as spent (one per debit in the transaction)

If any step fails, none of the changes must be visible. This is typically implemented using a database transaction.

#### `load_transactions`

- Returns all committed transactions in append order (the order they were committed)
- Used for replaying history or auditing

#### `tx_count`

- Returns the total number of committed transactions

## MemoryStorage

An in-memory implementation provided for testing and single-process use cases.

### Structure

```rust
struct MemoryState {
    assets: HashMap<String, Asset>,
    transactions: Vec<Transaction>,
    tokens: HashMap<(String, u32), SpendingToken>,  // keyed by (tx_id, entry_index)
    idempotency_keys: HashSet<String>,
}

pub struct MemoryStorage {
    state: RwLock<MemoryState>,
}
```

### Characteristics

- **Thread-safe**: Uses `RwLock` for concurrent access
- **No durability**: Data is lost when the process exits
- **No size limits**: All data held in memory
- **Suitable for**: Unit tests, integration tests, prototyping

### Usage

```rust
use ledger_core::MemoryStorage;
use std::sync::Arc;

let storage = Arc::new(MemoryStorage::new());
let ledger = Ledger::new(storage);
```

## Conformance Test Suite

The `storage::test_support` module provides a comprehensive test suite that any `Storage` implementation must pass. It is gated behind the `test-support` feature flag.

### Using the Test Suite

In your storage crate's `Cargo.toml`:

```toml
[dev-dependencies]
ledger-core = { path = "../ledger-core", features = ["test-support"] }
```

In your test file:

```rust
#[cfg(test)]
mod tests {
    use ledger_core::storage::test_support::storage_tests;

    storage_tests!(async { MyStorage::connect(":memory:").await });
}
```

The `storage_tests!` macro generates 45+ test functions covering:

| Category | Tests |
|----------|-------|
| **Assets** | Empty load, save/load, duplicate no-op, conflict rejection, multiple assets |
| **Idempotency** | Empty check, key recorded after commit, absent for uncommitted |
| **Token queries** | Nonexistent lookup, lookup after commit, spent status |
| **Unspent by account** | Empty, matching, excludes spent, excludes other assets, excludes children |
| **Unspent by prefix** | Empty, includes descendants, includes exact, excludes spent, excludes other assets, excludes non-descendants |
| **Transactions** | Empty load, empty count, commit and load, order preservation |
| **Atomicity** | Token and key creation, spend and create in same commit |
| **Balances by prefix** | Empty, grouping by account/asset, summing multiple tokens, excludes spent, excludes non-descendants, omits zero balances |

### Implementing a New Backend

To create a new storage backend:

1. Implement the `Storage` trait on your type
2. Add the conformance test suite via `storage_tests!`
3. Ensure `commit_tx` is atomic (use database transactions or equivalent)
4. Handle concurrent access safely (the trait requires `Send + Sync`)

The conformance suite is the source of truth for correctness -- if all tests pass, your implementation is compatible with `Ledger`.
