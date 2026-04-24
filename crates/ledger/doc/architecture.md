# Architecture

## Relationship to ledger-core

The `ledger` crate is a thin, ergonomic wrapper around `ledger-core`. It adds three capabilities that `ledger-core` deliberately omits:

1. **Automatic token selection** -- callers specify "spend X from account A" and the builder figures out which tokens to consume
2. **Pluggable debt strategies** -- different representations of debt (signed positions vs. split assets) without changing core ledger invariants
3. **Pluggable issuance strategies** -- automatic `@world` balancing when minting new tokens

All core functionality -- transaction validation, token management, balance queries, storage -- is delegated to `ledger-core::Ledger` via the `inner` field.

## Ledger Struct

```rust
pub struct Ledger {
    inner: ledger_core::Ledger,
    debt_strategy: Option<Arc<dyn DebtStrategy>>,
    issuance_strategy: Arc<dyn IssuanceStrategy>,
}
```

- `inner`: The core ledger engine. All query and commit operations delegate here.
- `debt_strategy`: Optional strategy for issuing and settling debt. When set, builders created by `transaction()` expose `create_debt()` and `settle_debt()`. When `None`, those builder methods return `Error::NoDebtStrategy`.
- `issuance_strategy`: Strategy for minting new tokens. Default: `TemplateIssuanceStrategy::new("@world")`.

### IssuanceStrategy

Every transaction must satisfy `sum(debits) == sum(credits)` per asset -- no exceptions. When minting new tokens, the `IssuanceStrategy` automatically creates a balancing negative credit at a source account (e.g., `@world`). The builder exposes two methods:

- `issue(to, amount)` -- uses the configured strategy (default: `@world`)
- `issue_from(source, to, amount)` -- uses a custom source (e.g., `"bank/chase"`, `"supplier/acme"`)

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
    .debit(account, asset, qty)     → TransactionBuilder (Self)
    .credit(to, asset, qty)         → TransactionBuilder (Self)
    .build().await                  → Transaction
```

Both `.credit()` and `.debit()` return `Self` (not `Result`), so no `.unwrap()` or `?` is needed on those calls.

Issuance and debt operations are part of the same fluent chain:

```
let tx = ledger.transaction("key")
    .issue("store/inventory", &brush_amount)?
    .debit("store/inventory", &brush_amount)
    .credit("customer/1", &brush_amount)
    .create_debt("customer-1", &gs_amount)?
    .build().await?;
```

This ensures the builder is always in a consistent state and prevents accidental reuse.

## Module Dependency Graph

```
ledger (this crate)
  ├── ledger.rs       → wraps ledger_core::Ledger
  ├── builder.rs      → wraps ledger_core::TransactionBuilder + token selection
  ├── error.rs        → extends LedgerError with high-level variants
  ├── issuance/
  │   ├── mod.rs      → IssuanceStrategy trait
  │   └── template.rs → TemplateIssuanceStrategy impl
  └── debt/
      ├── mod.rs      → DebtStrategy trait
      ├── signed_position.rs → SignedPositionDebt impl
      └── split_asset.rs     → SplitAssetDebt impl
          │
          ▼
      ledger-core (dependency)
        ├── Ledger, TransactionBuilder, Transaction
        ├── Storage, MemoryStorage
        ├── Asset, Amount, SpendingToken
        └── LedgerError
```
