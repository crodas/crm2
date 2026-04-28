# ledger-core

Low-level append-only UTXO ledger engine for modeling the movement of value -- inventory, cash, receivables, and debt -- using spending tokens and string account identifiers.

## Overview

`ledger-core` provides the foundational primitives for a double-entry accounting system built on the UTXO (Unspent Transaction Output) model. Every transaction consumes prior spending tokens (debits) and produces new ones (credits). Tokens are immutable once created; spending is tracked via status transitions.

## Key Concepts

- **Spending Tokens**: Immutable units of value owned by an account. Once spent, they cannot be reused (double-spend prevention).
- **Accounts**: String identifiers (e.g., `store1/inventory`, `customer1/cash`) with optional template-based aliases for transparent account resolution.
- **Assets**: Named quantities with configurable decimal precision and signedness. Signed assets support negative quantities for debt modeling.
- **Transactions**: Atomic operations that consume existing tokens and produce new ones, enforcing per-asset conservation invariants.
- **Deterministic IDs**: Transaction IDs are derived from a canonical preimage via double SHA-256, making them reproducible and tamper-evident.

## Architecture

```
ledger-core/
  src/
    lib.rs              # Public API re-exports
    ledger.rs           # Core engine: commit, balance, query
    transaction.rs      # Transaction types and builder with validation
    asset.rs            # Asset definitions, quantity parsing
    account.rs          # Account module (conventions only)
    alias.rs            # AliasRegistry: template-based account aliases
    token.rs            # SpendingToken, EntryRef, BalanceEntry
    error.rs            # LedgerError enum
    storage/
      mod.rs            # Storage trait + MemoryStorage implementation
      test_support.rs   # Generic conformance test suite (storage_tests! macro)
      tests.rs          # MemoryStorage test harness
```

## Usage

```rust
use ledger_core::{Ledger, Asset, AssetKind, MemoryStorage, TransactionBuilder};
use std::sync::Arc;

let storage = Arc::new(MemoryStorage::new());
let ledger = Ledger::new(storage);

// Register assets
let usd = Asset::new("usd", 2, AssetKind::Signed);
ledger.register_asset(usd).await?;

// Issue tokens (credit-only transaction)
let tx = TransactionBuilder::new("issue-001")
    .credit("store/cash", "usd", "100.00")
    .build(&ledger.assets())?;
ledger.commit(tx).await?;

// Query balances
let balance = ledger.balance("store/cash", "usd").await?;
```

## Documentation

See [`doc/`](doc/) for detailed technical documentation:

- [Architecture](doc/architecture.md) -- design principles, UTXO model, data flow
- [Assets & Quantities](doc/assets.md) -- asset kinds, precision, quantity parsing
- [Accounts](doc/accounts.md) -- path hierarchy, global queries, account aliases
- [Transactions](doc/transactions.md) -- builder pattern, validation rules, ID derivation
- [Tokens](doc/tokens.md) -- spending tokens, status lifecycle, balance entries
- [Storage](doc/storage.md) -- trait contract, MemoryStorage, conformance tests
- [Error Handling](doc/errors.md) -- every error variant and when it occurs
