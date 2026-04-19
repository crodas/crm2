# ledger

High-level UTXO ledger with automatic token selection and pluggable debt strategies. Builds on `ledger-core` to provide ergonomic transaction building and debt management.

## Overview

While `ledger-core` requires callers to manually reference specific spending tokens by their entry refs, `ledger` automates this with greedy token selection. It also introduces a `DebtStrategy` trait that decouples debt representation from the core ledger invariants.

## Key Features

- **Automatic Token Selection**: Specify an account, asset, and amount; the builder queries unspent tokens, selects greedily (largest first), and generates change credits automatically.
- **Pluggable Debt Strategies**: Two built-in strategies for modeling debt, with a trait for custom implementations.
- **Thin Wrapper**: All core query and commit operations delegate directly to `ledger-core`, adding no overhead.

## Architecture

```
ledger/
  src/
    lib.rs              # Public API and re-exports from ledger-core
    ledger.rs           # Ledger struct wrapping ledger-core with debt support
    builder.rs          # TransactionBuilder with auto token selection
    error.rs            # High-level Error enum
    debt/
      mod.rs            # DebtStrategy trait definition
      signed_position.rs  # SignedPositionDebt strategy
      split_asset.rs      # SplitAssetDebt strategy
```

## Usage

```rust
use ledger::{Ledger, Asset, AssetKind, MemoryStorage};
use ledger::debt::SignedPositionDebt;
use std::sync::Arc;

let storage = Arc::new(MemoryStorage::new());
let mut ledger = Ledger::new(storage);
ledger.with_debt_strategy(SignedPositionDebt);

// Register assets
ledger.register_asset(Asset::new("usd", 2, AssetKind::Signed)).await?;
ledger.register_asset(Asset::new("brush", 0, AssetKind::Unsigned)).await?;

// Build transaction with auto token selection
let tx = ledger.transaction("sale-001")
    .debit("@store/inventory", "brush", 5)
    .credit("@customer/goods", "brush", "5")
    .build().await?;
ledger.commit(tx).await?;

// Issue and settle debt
let mut builder = ledger.transaction("credit-sale-001");
builder = ledger.issue_debt(builder, &debtor, &creditor, &usd_asset, 5000)?;
let tx = builder.build().await?;
ledger.commit(tx).await?;
```

## Debt Strategies

### SignedPositionDebt

Uses positive/negative credits on the **same asset** to represent debt. Simple and queryable, but tokens accumulate without bound since they are never consumed.

### SplitAssetDebt

Creates a separate `{asset}.d` debt asset. Debt tokens are consumed via UTXO debits during settlement, providing double-spend protection and bounded token counts.

See [`doc/`](doc/) for a detailed comparison.

## Documentation

See [`doc/`](doc/) for detailed technical documentation:

- [Architecture](doc/architecture.md) -- how ledger wraps ledger-core, design decisions
- [Token Selection](doc/token-selection.md) -- algorithm, change generation, edge cases
- [Debt Strategies](doc/debt-strategies.md) -- trait design, SignedPosition vs SplitAsset comparison
- [Error Handling](doc/errors.md) -- error variants and recovery
