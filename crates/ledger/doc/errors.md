# Error Handling

## Error Enum

The `ledger` crate defines its own `Error` enum that extends `LedgerError` from `ledger-core` with high-level business errors.

```rust
pub enum Error {
    InsufficientBalance {
        account: String,
        asset: String,
        required: i128,
        available: i128,
    },
    NonPositiveAmount,
    InsufficientDebt {
        required: i128,
        available: i128,
    },
    NoDebtStrategy,
    Ledger(#[from] LedgerError),
}
```

### Variants

#### `InsufficientBalance`

Raised by `TransactionBuilder::build()` when token selection cannot find enough unspent tokens to cover a debit request.

```rust
// @store/cash has 50.00 usd
ledger.transaction("tx")
    .debit("@store/cash", "usd", 10000)  // Need 100.00
    .build().await;
// => Err(InsufficientBalance {
//     account: "@store/cash",
//     asset: "usd",
//     required: 10000,
//     available: 5000,
// })
```

#### `NonPositiveAmount`

Raised by debt strategies when the amount is zero or negative. Both `issue()` and `settle()` require strictly positive amounts.

```rust
ledger.transaction("tx")
    .create_debt(&debtor, &creditor, &usd, 0);
// => Err(NonPositiveAmount)

ledger.transaction("tx")
    .create_debt(&debtor, &creditor, &usd, -100);
// => Err(NonPositiveAmount)
```

#### `InsufficientDebt`

Raised by `SplitAssetDebt::settle()` when the debtor doesn't owe enough or the creditor isn't owed enough to cover the settlement amount.

```rust
// Customer owes 50.00 on usd.d
ledger.transaction("tx")
    .settle_debt(&debtor, &creditor, &usd, 10000).await;
// => Err(InsufficientDebt { required: 10000, available: 5000 })
```

#### `NoDebtStrategy`

Raised by `TransactionBuilder::create_debt()` or `TransactionBuilder::settle_debt()` when no debt strategy has been configured on the ledger.

```rust
let ledger = Ledger::new(storage);  // No strategy set
ledger.transaction("tx")
    .create_debt(&debtor, &creditor, &usd, 5000);
// => Err(NoDebtStrategy)
```

#### `Ledger(LedgerError)`

Wraps any `LedgerError` from `ledger-core`. This variant is automatically produced via `#[from]`, so `?` propagation works seamlessly:

```rust
// Core errors propagate through:
ledger.commit(tx).await?;  // LedgerError::AlreadySpent → Error::Ledger(AlreadySpent)
```

## Error Recovery Patterns

### Retry on Insufficient Balance

If token selection fails, the caller can check the available amount and adjust:

```rust
match builder.build().await {
    Err(Error::InsufficientBalance { available, .. }) => {
        // Offer partial amount or ask user
    }
    _ => {}
}
```

### Retry on Insufficient Debt

For settlements that exceed the outstanding debt:

```rust
match builder.settle_debt(&debtor, &creditor, &usd, amount).await {
    Err(Error::InsufficientDebt { available, .. }) => {
        // Settle only the available amount instead
    }
    _ => {}
}
```
