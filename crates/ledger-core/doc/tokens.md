# Spending Tokens

## Overview

Spending tokens are the fundamental units of value in the UTXO ledger. Each token represents a discrete quantity of an asset owned by a specific account. Tokens are immutable -- once created, their owner, asset, and quantity never change. The only mutable state is the token's spend status.

## Types

### EntryRef

A unique reference to a specific credit entry in a committed transaction:

```rust
pub struct EntryRef {
    pub tx_id: String,       // Transaction that created this entry
    pub entry_index: u32,    // Zero-based position in that transaction's credits
}
```

The `(tx_id, entry_index)` pair is globally unique across the entire ledger. It serves as the token's primary key.

**Display format:** `"<first-8-chars-of-tx_id>:<entry_index>"`
```rust
// EntryRef { tx_id: "abcdef1234567890...", entry_index: 0 }
// displays as: "abcdef12:0"
```

### TokenStatus

```rust
pub enum TokenStatus {
    Unspent,        // Available for consumption in a future transaction
    Spent(usize),   // Consumed by the transaction at this index in the ledger
}
```

A token starts as `Unspent` when its parent transaction is committed. When a subsequent transaction references it as a debit, the token transitions to `Spent(tx_index)` where `tx_index` is the ordinal position of the spending transaction in the append-only ledger.

This transition is **irreversible** -- a spent token cannot be unspent. Attempting to debit a spent token returns `LedgerError::AlreadySpent`.

### SpendingToken

The complete token with all metadata:

```rust
pub struct SpendingToken {
    pub entry_ref: EntryRef,     // Where this token was created
    pub owner: AccountPath,      // Account that owns it
    pub asset_name: String,      // Asset name
    pub qty: i128,               // Quantity (scaled by asset precision)
    pub status: TokenStatus,     // Unspent or Spent
}
```

**Key properties:**
- `owner` is set at creation and never changes
- `qty` can be negative for signed assets (representing debt obligations)
- `status` is the only field that transitions (Unspent -> Spent)

### BalanceEntry

An aggregated view of unspent tokens grouped by account and asset:

```rust
pub struct BalanceEntry {
    pub account: AccountPath,    // Account owning tokens
    pub asset_name: String,      // Asset name
    pub balance: i128,           // Sum of unspent token quantities
}
```

Returned by `balances_by_prefix()` queries. Zero-balance entries are excluded.

## Token Lifecycle

```
Transaction committed
    with credit to @store/cash for 100.00 usd
        │
        ▼
┌─────────────────┐
│  SpendingToken   │
│  entry_ref: (tx1, 0)  │
│  owner: @store/cash    │
│  asset: usd            │
│  qty: 10000            │
│  status: Unspent       │
└────────┬────────┘
         │
         │  Referenced as debit in a new transaction
         │
         ▼
┌─────────────────┐
│  SpendingToken   │
│  entry_ref: (tx1, 0)  │
│  owner: @store/cash    │
│  asset: usd            │
│  qty: 10000            │
│  status: Spent(1)      │  ← Now spent by tx at index 1
└─────────────────┘
```

## Query Methods

### Single Account Queries

```rust
// All unspent tokens for a specific account and asset
let tokens: Vec<SpendingToken> = ledger.unspent_tokens(
    &AccountPath::new("@store/cash")?,
    "usd"
).await?;

// Balance (sum of unspent token quantities)
let balance: i128 = ledger.balance(
    &AccountPath::new("@store/cash")?,
    "usd"
).await?;
```

Single account queries match **exactly** -- `@store` does not include `@store/cash`.

### Prefix Queries

```rust
// All unspent tokens under a prefix for a specific asset
let tokens = ledger.unspent_tokens_prefix(
    &AccountPath::new("@store")?,
    "usd"
).await?;
// Includes: @store, @store/cash, @store/receivables/sale_1, etc.

// All unspent tokens under a prefix across ALL assets
let all = ledger.unspent_all_by_prefix(
    &AccountPath::new("@store")?
).await?;

// Aggregated prefix balance for one asset
let total = ledger.balance_prefix(
    &AccountPath::new("@store")?,
    "usd"
).await?;

// Grouped balances by (account, asset) under prefix
let entries: Vec<BalanceEntry> = ledger.balances_by_prefix(
    &AccountPath::new("@store")?
).await?;
// Returns: [
//   BalanceEntry { account: @store/cash, asset: "usd", balance: 10000 },
//   BalanceEntry { account: @store/inventory, asset: "brush", balance: 50 },
// ]
```

Prefix queries include the exact account **and** all descendants (paths starting with `prefix/`).

## Negative Tokens

For signed assets, tokens can have negative quantities. These represent obligations or debt positions:

```rust
// A credit sale creates a negative token (debt) and a positive token (receivable)
TransactionBuilder::new("credit-sale")
    .credit("@customer/debt", "usd", "-50.00")       // qty = -5000
    .credit("@store/receivable", "usd", "50.00")      // qty = 5000
    .build(&assets)?;
```

Negative tokens:
- Are valid only for `AssetKind::Signed` assets
- Participate in balance calculations (a -50.00 and +30.00 token sum to -20.00)
- Can be consumed as debits, just like positive tokens
- Settlement involves debiting both the negative and positive tokens

## Token Counting & Fragmentation

Each credit in a committed transaction creates exactly one token. Over time, patterns like change generation and partial settlements create many small tokens. This is called **UTXO fragmentation**.

For example, if you receive 100 units as one token and spend 30, you get:
- Original 100-unit token: Spent
- New 30-unit token: Unspent (sent to recipient)
- New 70-unit token: Unspent (change back to sender)

Now spending 20 from the 70-unit change creates two more tokens, and so on. The `SplitAssetDebt` strategy (in the `ledger` crate) is particularly affected by this, as partial debt settlements create change tokens on both sides.
