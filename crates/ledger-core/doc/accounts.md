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
store1/receivables             # Receivables parent account
store1/receivables/sale_1      # Receivable for a specific sale
store1/receivables/sale_2      # Receivable for another sale

customer1                      # Customer top-level
customer1/cash                 # Customer's cash account
customer1/goods                # Goods received by customer
```

## Issuance

Issuance is represented by credit-only transactions (transactions with no debits). These create tokens from nothing -- no special source account is needed. The conservation check is skipped for issuance transactions.

## Listing Accounts

The ledger provides an `accounts()` method that returns all distinct account names with unspent tokens.

```rust
let accts: Vec<String> = ledger.accounts().await?;
// Returns: ["store1/cash", "store1/inventory", "customer1/cash"]
```

To query a specific account's balance or tokens, use the per-account methods:

```rust
let bal: i128 = ledger.balance("store1/cash", "usd").await?;
let tokens: Vec<SpendingToken> = ledger.unspent_tokens("store1/cash", "usd").await?;
```

## Account Aliases

The `AliasRegistry` (in `ledger-core::alias`) provides template-based account aliases that are resolved transparently before querying storage. This is a pure resolution layer in the `Ledger` -- the storage layer has no alias awareness.

### Template Syntax

Templates use `{name}` placeholders that match a single path segment. Both sides of a rule must declare the same set of placeholders.

```rust
let mut aliases = AliasRegistry::new();
aliases.register(
    "user/{user_id}/to_pay/{sale_id}",     // canonical form
    "sale/{sale_id}/receivables/{user_id}", // alias form
).unwrap();
```

### Resolution

`resolve(account)` converts an alias-form path to canonical form. If no rule matches, the input is returned unchanged.

```rust
aliases.resolve("sale/1/receivables/42")  // → "user/42/to_pay/1"
aliases.resolve("warehouse/1")            // → "warehouse/1" (no match)
```

### Integration with Ledger

The `Ledger` holds an optional `AliasRegistry` set via `with_aliases()`. Account arguments to `balance()`, `unspent_tokens()`, and similar methods are resolved through the registry before hitting storage.

```rust
let ledger = Ledger::new(storage).with_aliases(aliases);

// Both of these query the same canonical account:
ledger.balance("user/42/to_pay/1", "usd").await?;
ledger.balance("sale/1/receivables/42", "usd").await?;
```
