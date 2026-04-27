# Accounts

## Account Identifiers

Accounts are plain strings (`&str` / `String`) that serve as identifiers in the ledger. There is no special wrapper type or prefix requirement.

### Examples

```rust
// Valid account strings
"store1"
"store1/inventory"
"customer1/cash"
"store1/receivables/sale_42"
```

## Hierarchy & Naming Conventions

Accounts use `/` as a path separator to form a logical hierarchy. The hierarchy is a **naming convention** -- the ledger does not enforce parent-child relationships or create intermediate nodes. Any valid string can hold tokens independently.

### Common Patterns

```
store1                         # Top-level store account
store1/inventory               # Inventory sub-account
store1/cash                    # Cash holdings
store1/receivables             # Aggregate receivables prefix
store1/receivables/sale_1      # Receivable for a specific sale
store1/receivables/sale_2      # Receivable for another sale

customer1                      # Customer top-level
customer1/cash                 # Customer's cash account
customer1/goods                # Goods received by customer
```

## Issuance

Issuance is represented by credit-only transactions (transactions with no debits). These create tokens from nothing -- no special source account is needed. The conservation check is skipped for issuance transactions.

## Prefix Queries

The prefix-based query methods enable hierarchical aggregation.

### Prefix Matching

An account A is a prefix of account B if B starts with A followed by `/`:

```rust
// "store1" is a prefix of "store1/inventory" and "store1/cash"
// "store1" is NOT a prefix of "store2"
// "store1/inventory" is NOT a prefix of "store1" (child is not prefix of parent)
```

### Prefix-Based Ledger Queries

```rust
// Balance across all accounts under store1
let total = ledger.balance_prefix("store1", "usd").await?;

// All unspent credit tokens under store1 for a specific asset
let usd = Asset::new("usd", 2);
let tokens = ledger.unspent_tokens_prefix("store1", Some(&usd.max())).await?;

// All unspent credit tokens under store1 across all assets
let all = ledger.unspent_tokens_prefix("store1", None).await?;

// Aggregated balances grouped by (account, asset)
let entries = ledger.balances_by_prefix("store1").await?;
```

Prefix queries always include the exact account itself **and** all descendants. For example, querying `store1` returns tokens owned by `store1`, `store1/cash`, `store1/inventory`, `store1/receivables/sale_1`, etc.
