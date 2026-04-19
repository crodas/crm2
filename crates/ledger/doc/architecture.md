# Architecture

## Relationship to ledger-core

The `ledger` crate is a thin, ergonomic wrapper around `ledger-core`. It adds two capabilities that `ledger-core` deliberately omits:

1. **Automatic token selection** -- callers specify "spend X from account A" and the builder figures out which tokens to consume
2. **Pluggable debt strategies** -- different representations of debt (signed positions vs. split assets) without changing core ledger invariants

All core functionality -- transaction validation, token management, balance queries, storage -- is delegated to `ledger-core::Ledger` via the `inner` field.

## Ledger Struct

```rust
pub struct Ledger {
    inner: ledger_core::Ledger,
    debt_strategy: Option<Box<dyn DebtStrategy>>,
}
```

- `inner`: The core ledger engine. All query and commit operations delegate here.
- `debt_strategy`: Optional strategy for issuing and settling debt. When `None`, calls to `issue_debt()` / `settle_debt()` return `Error::NoDebtStrategy`.

## Design Decisions

### Why Wrap Instead of Extend?

The core ledger enforces universal invariants (conservation, double-spend prevention) that must hold regardless of how transactions are constructed. Token selection and debt modeling are higher-level concerns that belong in a separate layer:

- Token selection needs storage access to query unspent tokens, which `TransactionBuilder` in core deliberately avoids (it's storage-agnostic)
- Debt strategies inject domain-specific credits/debits that the core doesn't need to understand
- Different applications may want different token selection algorithms or debt models

### Why Optional Debt Strategy?

Not all uses of the ledger involve debt. Inventory tracking, for example, only needs issuance and transfer. Making the strategy optional avoids forcing callers to choose a debt model when they don't need one.

### Builder Ownership Flow

The `TransactionBuilder` follows a linear ownership pattern:

```
ledger.transaction("key")           → TransactionBuilder
    .debit(account, asset, qty)     → TransactionBuilder (moved)
    .credit(to, asset, qty)         → TransactionBuilder (moved)
    .build().await                  → Transaction
```

For debt operations, the builder is passed into the strategy and returned:

```
let mut builder = ledger.transaction("key");
builder = ledger.issue_debt(builder, debtor, creditor, asset, amount)?;
// builder is moved into issue_debt and returned with debt entries added
let tx = builder.build().await?;
```

This ensures the builder is always in a consistent state and prevents accidental reuse.

## Module Dependency Graph

```
ledger (this crate)
  ├── ledger.rs       → wraps ledger_core::Ledger
  ├── builder.rs      → wraps ledger_core::TransactionBuilder + token selection
  ├── error.rs        → extends LedgerError with high-level variants
  └── debt/
      ├── mod.rs      → DebtStrategy trait
      ├── signed_position.rs → SignedPositionDebt impl
      └── split_asset.rs     → SplitAssetDebt impl
          │
          ▼
      ledger-core (dependency)
        ├── Ledger, TransactionBuilder, Transaction
        ├── Storage, MemoryStorage
        ├── Asset, AccountPath, SpendingToken
        └── LedgerError
```
