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
    async fn get_credit_token(&self, eref: &CreditEntryRef) -> Result<Option<CreditToken>, LedgerError>;
    async fn unspent_by_account(&self, account: &str, requested_amount: Option<&Amount>)
        -> Result<Vec<CreditToken>, LedgerError>;
    async fn unspent_by_prefix(&self, prefix: &str, requested_amount: Option<&Amount>)
        -> Result<Vec<CreditToken>, LedgerError>;
    async fn balances_by_prefix(&self, prefix: &str)
        -> Result<Vec<BalanceEntry>, LedgerError>;

    // Granular write primitives (composed by the saga layer)
    async fn mark_spent(&self, refs: &[CreditEntryRef], by_tx: &str) -> Result<(), LedgerError>;
    async fn unmark_spent(&self, refs: &[CreditEntryRef], tx_to_revert: &str) -> Result<(), LedgerError>;
    async fn insert_credit_tokens(&self, tokens: &[CreditToken]) -> Result<(), LedgerError>;
    async fn remove_credit_tokens(&self, refs: &[CreditEntryRef]) -> Result<(), LedgerError>;
    async fn insert_tx(&self, tx: &Transaction) -> Result<(), LedgerError>;
    async fn remove_tx(&self, tx_id: &str) -> Result<(), LedgerError>;

    // Transaction queries
    async fn load_transactions(&self) -> Result<Vec<Transaction>, LedgerError>;
    async fn tx_count(&self) -> Result<usize, LedgerError>;
}
```

### Write Primitives & Saga

Write operations are granular primitives. The saga layer in `crate::saga` composes them into an all-or-nothing commit with automatic compensation on failure:

1. **Mark spent** — flag input tokens as consumed (`mark_spent`)
2. **Create tokens** — insert new output tokens (`insert_credit_tokens`)
3. **Insert transaction** — persist the transaction record (`insert_tx`)

If any step fails, completed steps are compensated in reverse order using `unmark_spent`, `remove_credit_tokens`, and `remove_tx`. Each write method should wrap its operations in a database transaction for atomicity.

### Method Contracts

#### `register_asset`

- Must persist the asset definition durably
- **Idempotent**: registering the same asset (name, precision) twice is a silent no-op
- Must return `AssetConflict` if the name exists with different precision

#### `has_idempotency_key`

- Returns `true` if a transaction with this key has been committed
- Must be consistent with `insert_tx` -- if `insert_tx` succeeds, all subsequent calls must return `true` for that key

#### `get_credit_token`

- Returns `Some(CreditToken)` if a credit token exists at the given `CreditEntryRef`
- Returns `None` if no credit token exists at that reference
- Must reflect the current spend status (Unspent or Spent)

#### `unspent_by_account`

- `Some(amount)` — returns all credit tokens with `status == Unspent` for the **exact** account and the amount's asset
- `None` — returns all unspent credit tokens for the account across all assets
- Must **not** include descendant accounts (e.g., `store` does not include `store/cash`)

#### `unspent_by_prefix`

- `Some(amount)` — returns all credit tokens with `status == Unspent` where the owner matches the prefix **or** is a descendant, filtered by the amount's asset
- `None` — same but across all assets
- Prefix matching: owner == prefix OR owner starts with `{prefix}/`

#### `balances_by_prefix`

- Returns aggregated sums grouped by (account, asset)
- Only includes groups with non-zero balances
- Sorted by account, then asset name

#### `mark_spent`

- Marks the given tokens as spent by `by_tx`
- Each referenced token must exist and be unspent
- Should use a CAS guard (e.g., `WHERE spent_by_tx IS NULL`) and return `AlreadySpent` if a credit token was already consumed

#### `unmark_spent`

- Compensation: restores previously-spent credit tokens to unspent
- Only reverts tokens whose `spent_by_tx` matches `tx_to_revert`, leaving tokens spent by other transactions untouched

#### `insert_credit_tokens` / `remove_credit_tokens`

- Insert or remove credit tokens by their entry references
- Used as execute/compensate pair in the saga

#### `insert_tx` / `remove_tx`

- Insert or remove a transaction record and its idempotency key
- Used as execute/compensate pair in the saga

#### `load_transactions`

- Returns all committed transactions in append order (the order they were committed)

#### `tx_count`

- Returns the total number of committed transactions

## MemoryStorage

An in-memory implementation provided for testing and single-process use cases.

### Structure

```rust
struct MemoryState {
    assets: HashMap<String, Asset>,
    transactions: Vec<Transaction>,
    credit_tokens: HashMap<CreditEntryRef, CreditToken>,
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
    use ledger_core::storage_tests;

    storage_tests!(async { MyStorage::connect(":memory:").await });
}
```

The `storage_tests!` macro generates 40+ test functions covering:

| Category | Tests |
|----------|-------|
| **Assets** | Empty load, save/load, duplicate no-op, conflict rejection, multiple assets |
| **Idempotency** | Empty check, key recorded after commit, absent for uncommitted |
| **Token queries** | Nonexistent lookup, lookup after commit, spent status |
| **Unspent by account** | Empty, matching, excludes spent, excludes other assets, excludes children |
| **Unspent by prefix** | Empty, includes descendants, includes exact, excludes spent, excludes other assets, excludes non-descendants |
| **Transactions** | Empty load, empty count, commit and load, order preservation |
| **Write primitives** | mark_spent flags tokens, unmark_spent restores, insert/remove tokens, insert/remove tx |
| **Balances by prefix** | Empty, grouping by account/asset, summing multiple tokens, excludes spent, excludes non-descendants, omits zero balances |

### Implementing a New Backend

To create a new storage backend:

1. Implement the `Storage` trait on your type
2. Add the conformance test suite via `storage_tests!`
3. Wrap each write method in a database transaction for atomicity
4. Handle concurrent access safely (the trait requires `Send + Sync`)

The conformance suite is the source of truth for correctness -- if all tests pass, your implementation is compatible with `Ledger`.
