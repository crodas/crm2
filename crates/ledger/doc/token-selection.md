# Token Selection

## Overview

When building a transaction with `ledger::TransactionBuilder`, callers specify "I want to debit X units of asset A from account B" without knowing which specific tokens to consume. The builder handles token selection automatically.

## Algorithm

The token selection algorithm is **greedy largest-first**:

1. **Query** all unspent tokens for the specified account and asset from storage
2. **Sort** tokens by quantity in descending order (largest first)
3. **Accumulate** tokens until the sum meets or exceeds the requested amount
4. **Generate change** if the accumulated sum exceeds the requested amount

```
Unspent tokens for @store/cash (usd):
  Token A: 5000  (50.00)
  Token B: 3000  (30.00)
  Token C: 1000  (10.00)

Request: debit 7000 (70.00)

Step 1: Sort → [A=5000, B=3000, C=1000]
Step 2: Take A (total=5000, need 7000) → not enough
Step 3: Take B (total=8000, need 7000) → enough!
Step 4: Change = 8000 - 7000 = 1000 → credit @store/cash for 1000
```

### Why Largest-First?

- **Minimizes token count**: Fewer tokens selected means fewer debit entries in the transaction
- **Reduces fragmentation**: Using large tokens first tends to consolidate value
- **Simple and predictable**: Easy to reason about and debug

### Limitations

- Not optimal for minimizing change in all cases (that would require a knapsack solver)
- Does not consider token age or other FIFO/LIFO ordering
- Sufficient for the CRM use case where token count per account is typically small

## API

### High-Level Debit

```rust
let builder = ledger.transaction("sale-001")
    .debit("@store/inventory", "brush", 5)   // Auto token selection
    .credit("@customer/goods", "brush", "5");
```

The `debit()` method records a `DebitRequest` (account, asset, quantity). Actual token selection happens during `build()`.

### Raw Debit (Bypass Token Selection)

```rust
let builder = ledger.transaction("settle-001")
    .debit_raw(&tx_id, 0, "@customer/debt", "usd", "-50.00");  // Explicit token ref
```

The `debit_raw()` method adds a pre-selected debit that bypasses token selection. This is used by debt strategies that need to reference specific tokens (e.g., `SplitAssetDebt` consuming specific debt tokens).

### Build

```rust
let tx: Transaction = builder.build().await?;
```

During `build()`:

1. For each `DebitRequest`: query storage, run token selection, add debits + change
2. For each raw debit: add directly to the low-level builder
3. Add all credits to the low-level builder
4. Delegate to `ledger_core::TransactionBuilder::build()` for validation

## Change Generation

When selected tokens sum to more than the requested amount, the builder automatically creates a **change credit** back to the source account.

```rust
// Token at @store/cash has qty 10000 (100.00 usd)
ledger.transaction("tx-001")
    .debit("@store/cash", "usd", 7000)      // Need 70.00
    .credit("@vendor/cash", "usd", "70.00")
    .build().await?;

// Result transaction:
//   Debit: (token_tx_id, 0) from @store/cash for 100.00
//   Credit: @vendor/cash  70.00
//   Credit: @store/cash   30.00  ← auto-generated change
```

Change is credited to the **same account** that was debited. The change credit is added before any user-specified credits, so the credit ordering is: change credits first, then user credits.

## Insufficient Balance

If the available unspent tokens don't cover the requested amount:

```rust
let result = ledger.transaction("tx-001")
    .debit("@store/cash", "usd", 100000)  // Need 1000.00
    .build().await;

// Err(Error::InsufficientBalance {
//     account: "@store/cash",
//     asset: "usd",
//     required: 100000,
//     available: 10000,
// })
```

## Multi-Asset Transactions

A single transaction can debit multiple accounts and assets. Each debit request runs token selection independently:

```rust
ledger.transaction("sale-001")
    .debit("@store/inventory", "brush", 5)   // Selects brush tokens
    .debit("@store/inventory", "paint", 2)   // Selects paint tokens independently
    .credit("@customer/goods", "brush", "5")
    .credit("@customer/goods", "paint", "2")
    .build().await?;
```

## Interaction with Debt Strategies

Debt strategies use the builder's `debit_raw()` and `credit()` methods to inject their entries. Debt operations are part of the builder's fluent API:

```rust
let tx = ledger.transaction("credit-sale")
    .debit("@store/inventory", "brush", 5)
    .credit("@customer/goods", "brush", "5")
    .create_debt(&debtor, &creditor, &usd, 5000)?
    .build().await?;
```

This design means token selection and debt entries coexist naturally in the same transaction.
