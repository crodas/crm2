# Accounts

## AccountPath

An `AccountPath` is a validated hierarchical identifier for an account in the ledger. It is a newtype wrapping a `String` with validation rules enforced at construction.

### Validation Rules

1. Must start with `@`
2. Must have at least one character after `@`
3. Cannot be `@world` (reserved for issuance)

```rust
// Valid paths
AccountPath::new("@store1")?;
AccountPath::new("@store1/inventory")?;
AccountPath::new("@customer1/cash")?;
AccountPath::new("@store1/receivables/sale_42")?;

// Invalid paths
AccountPath::new("store1");      // => Err(MissingPrefix)
AccountPath::new("@");           // => Err(Empty)
AccountPath::new("@world");      // => Err(ReservedWorld)
```

### InvalidAccountPath Errors

| Variant | Description |
|---------|-------------|
| `MissingPrefix(String)` | Path does not start with `@` |
| `Empty` | Nothing after the `@` prefix |
| `ReservedWorld` | `@world` is a pseudo-account, not a real account |

## Hierarchy & Naming Conventions

Accounts use `/` as a path separator to form a logical hierarchy. The hierarchy is a **naming convention** -- the ledger does not enforce parent-child relationships or create intermediate nodes. Any valid path can hold tokens independently.

### Common Patterns

```
@store1                         # Top-level store account
@store1/inventory               # Inventory sub-account
@store1/cash                    # Cash holdings
@store1/receivables             # Aggregate receivables prefix
@store1/receivables/sale_1      # Receivable for a specific sale
@store1/receivables/sale_2      # Receivable for another sale

@customer1                      # Customer top-level
@customer1/cash                 # Customer's cash account
@customer1/goods                # Goods received by customer
```

## The @world Pseudo-Account

`@world` is a reserved name that represents the "outside world." It is used conceptually as the source for issuance transactions (transactions with no debits). You cannot:

- Create an `AccountPath` for `@world`
- Use `@world` as a credit destination (returns `LedgerError::WorldAsOwner`)

Issuance is implicit: a transaction with credits but no debits creates tokens from nothing. The conservation check is skipped for issuance transactions.

## Prefix Queries

The `is_prefix_of` method and prefix-based query methods enable hierarchical aggregation.

### is_prefix_of

An account A is a prefix of account B if B's path starts with A's path followed by `/`:

```rust
let store = AccountPath::new("@store1")?;
let inv = AccountPath::new("@store1/inventory")?;
let cash = AccountPath::new("@store1/cash")?;
let other = AccountPath::new("@store2")?;

assert!(store.is_prefix_of(&inv));    // true
assert!(store.is_prefix_of(&cash));   // true
assert!(!store.is_prefix_of(&other)); // false
assert!(!inv.is_prefix_of(&store));   // false (child is not prefix of parent)
```

### Prefix-Based Ledger Queries

```rust
// Balance across all accounts under @store1
let total = ledger.balance_prefix(&store, "usd").await?;

// All unspent tokens under @store1 for a specific asset
let tokens = ledger.unspent_tokens_prefix(&store, "usd").await?;

// All unspent tokens under @store1 across all assets
let all = ledger.unspent_all_by_prefix(&store).await?;

// Aggregated balances grouped by (account, asset)
let entries = ledger.balances_by_prefix(&store).await?;
```

Prefix queries always include the exact account itself **and** all descendants. For example, querying `@store1` returns tokens owned by `@store1`, `@store1/cash`, `@store1/inventory`, `@store1/receivables/sale_1`, etc.

## Serde Support

`AccountPath` implements `Serialize` (via `From<AccountPath> for String`) and `Deserialize` (via `TryFrom<String>`). Deserialization validates the path and returns an error for invalid values.
