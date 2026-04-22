# Transactions

## Overview

A `Transaction` represents an atomic, validated movement of value in the ledger. It consumes zero or more existing spending tokens (debits) and produces one or more new tokens (credits). Transactions are built using `TransactionBuilder`, which enforces all structural invariants before producing a sealed `Transaction`.

## Types

### DebitRef

A reference to a prior credit entry being consumed:

```rust
pub struct DebitRef {
    pub tx_id: String,         // Transaction that created the token
    pub entry_index: u32,      // Position in that transaction's credits
    pub owner: String,         // Expected owner (verified at commit)
    pub asset_name: String,    // Expected asset (verified at commit)
    pub qty: String,           // Expected quantity (verified at commit)
}
```

The `owner`, `asset_name`, and `qty` fields are expectations -- they are checked against the actual token during `Ledger::commit()`. If any field doesn't match, the commit fails with a mismatch error. This prevents a class of bugs where a debit silently consumes the wrong token.

### Credit

A new spending token to be created:

```rust
pub struct Credit {
    pub to: String,            // Destination account
    pub asset_name: String,    // Asset name
    pub qty: String,           // Quantity (decimal string, may be negative for signed assets)
}
```

### Transaction

A sealed, validated transaction:

```rust
pub struct Transaction {
    pub tx_id: String,             // Deterministic ID (double SHA-256)
    pub idempotency_key: String,   // Caller-supplied unique key
    pub debits: Vec<DebitRef>,     // Consumed tokens (empty for issuance)
    pub credits: Vec<Credit>,      // New tokens produced
}
```

## TransactionBuilder

### Construction

```rust
let builder = TransactionBuilder::new("my-idempotency-key");
```

The idempotency key is a caller-chosen string that uniquely identifies this transaction. The ledger rejects transactions with duplicate keys, enabling safe retries.

### Adding Debits

```rust
builder.debit(&tx_id, entry_index, "store/inventory", "brush", "5")
```

Each debit references a specific credit from a prior committed transaction by its `(tx_id, entry_index)` pair. The remaining fields are expectations that are verified at commit time.

### Adding Credits

```rust
builder.credit("customer/goods", "brush", "5")
builder.credit("customer/debt", "usd", "-10.00")  // Negative for debt (signed assets only)
```

Credits create new spending tokens owned by the specified account. The `.credit()` and `.debit()` methods return `Self` for chaining (they are infallible).

### Building

```rust
let tx: Transaction = builder.build(&ledger.assets())?;
```

The `build()` method validates the transaction and computes the deterministic transaction ID. It requires the current asset registry to validate asset names, kinds, and quantity parsing.

## Validation Rules

### Structural Validation (at build time)

| Rule | Error |
|------|-------|
| All assets must be registered | `UnknownAsset` |
| Unsigned assets cannot have negative quantities | `NegativeUnsigned` |
| Quantity strings must parse correctly | `InvalidQty` |
| Per-asset conservation: sum(debits) == sum(credits) | `ConservationViolated` |
| Every negative credit must have a matching positive credit | `DanglingDebt` |

### Conservation Rule

For each asset present in the transaction:
- If there are debits for that asset: `sum(debit_qty) == sum(credit_qty)`
- If there are no debits for that asset: the transaction is issuance for that asset (conservation skipped)

This means a single transaction can issue some assets (no debits) while transferring others (with debits), as long as each transferred asset conserves independently.

### Dual-Sided Debt Rule

If any credit has a negative quantity, there must be at least one credit with a positive quantity for the same asset in the same transaction. This prevents creating "orphan" debt that isn't owed to anyone.

```rust
// Valid: debt has both sides
builder
    .credit("debtor/account", "usd", "-10.00")   // Debtor owes
    .credit("creditor/receivable", "usd", "10.00") // Creditor is owed

// Invalid: dangling debt (no positive side)
builder
    .credit("debtor/account", "usd", "-10.00")
// => Err(DanglingDebt { asset: "usd" })
```

### State Validation (at commit time)

| Rule | Error |
|------|-------|
| Referenced token must exist | `DebitNotFound` |
| Token must not be already spent | `AlreadySpent` |
| Token owner must match debit expectation | `DebitOwnerMismatch` |
| Token asset must match debit expectation | `DebitAssetMismatch` |
| Token quantity must match debit expectation | `DebitQtyMismatch` |
| Idempotency key must be unique | `DuplicateIdempotencyKey` |
| Computed tx_id must match stored tx_id | `TxIdMismatch` |

## Transaction ID Derivation

The transaction ID is computed deterministically from its contents:

```
tx_id = hex(SHA256(SHA256(canonical_preimage)))
```

### Canonical Preimage Format

The preimage is a null-byte (`\0`) delimited encoding:

```
D\0<tx_id>\0<entry_index>\0<owner>\0<asset>\0<qty>\0
D\0<tx_id>\0<entry_index>\0<owner>\0<asset>\0<qty>\0
...
C\0<to>\0<asset>\0<qty>\0
C\0<to>\0<asset>\0<qty>\0
...
K\0<idempotency_key>
```

- `D` prefix: debit entries
- `C` prefix: credit entries
- `K` prefix: idempotency key
- Declaration order is preserved (not sorted)

### Why Double SHA-256?

Double hashing prevents length-extension attacks. With a single SHA-256, an attacker who knows `H(m)` could compute `H(m || padding || suffix)` without knowing `m`. The outer hash eliminates this.

## Transaction Categories

### Issuance (No Debits)

Represents value entering the system. Used for inventory receipts, cash deposits, or initial balances. These are credit-only transactions with no debits.

```rust
TransactionBuilder::new("stock-receipt-001")
    .credit("store/inventory", "brush", "100")
    .build(&assets)?
```

### Transfer (Debits + Credits, Same Assets)

Moves value between accounts. Must conserve per asset.

```rust
TransactionBuilder::new("internal-transfer-001")
    .debit(&tx_id, 0, "store/cash", "usd", "50.00")
    .credit("store/petty_cash", "usd", "50.00")
    .build(&assets)?
```

### Transfer with Change

When a token is larger than needed, the excess is credited back to the source.

```rust
// Token at (tx_id, 0) has qty "100.00"
TransactionBuilder::new("transfer-with-change")
    .debit(&tx_id, 0, "store/cash", "usd", "100.00")
    .credit("vendor/cash", "usd", "30.00")
    .credit("store/cash", "usd", "70.00")   // Change back to source
    .build(&assets)?
```

### Credit Sale (Multi-Asset + Debt)

Transfers physical goods and creates a debt obligation in a single atomic transaction.

```rust
TransactionBuilder::new("credit-sale-001")
    .debit(&inv_tx, 0, "store/inventory", "brush", "5")
    .credit("customer/goods", "brush", "5")
    .credit("customer/debt", "usd", "-50.00")            // Customer owes
    .credit("store/receivables/sale_1", "usd", "50.00")   // Store is owed
    .build(&assets)?
```

### Debt Settlement

Consumes debt tokens and cash tokens to close out obligations.

```rust
TransactionBuilder::new("settle-001")
    .debit(&cash_tx, 0, "customer/cash", "usd", "50.00")
    .debit(&debt_tx, 0, "customer/debt", "usd", "-50.00")
    .debit(&recv_tx, 0, "store/receivables/sale_1", "usd", "50.00")
    .credit("store/cash", "usd", "50.00")
    .build(&assets)?
```

## Idempotency

The idempotency key is a caller-chosen string (typically a UUID or domain-specific key like `"sale-42"`) that prevents duplicate commits. If a transaction with the same key has already been committed, `Ledger::commit()` returns `DuplicateIdempotencyKey`.

This enables safe retries: if a network error occurs after a successful commit, the caller can retry with the same key and get a clear signal that the transaction was already applied.
