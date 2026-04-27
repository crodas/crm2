# Architecture

## Design Principles

### Append-Only Immutability

Every transaction, once committed, is permanent. There are no updates or deletions -- only new transactions that consume prior tokens and produce new ones. This guarantees a complete audit trail and eliminates an entire class of consistency bugs.

### UTXO Model

Inspired by Bitcoin's transaction model, `ledger-core` tracks value as discrete credit tokens rather than mutable account balances. Each token:

- Is created by exactly one transaction (as a credit entry)
- Can be spent by at most one subsequent transaction (as a debit entry)
- Has an immutable owner, asset, and quantity

Balances are derived by summing unspent credit tokens, never stored directly. This means the balance is always consistent with the transaction history -- there is no possibility of a balance drifting out of sync with its underlying entries.

### Conservation Law

Every non-issuance transaction must conserve value: for each asset, the sum of debited quantities must equal the sum of credited quantities. This is enforced at `TransactionBuilder::build()` time, before the transaction reaches the storage layer.

Issuance transactions (those with no debits) are exempt -- they represent value entering the system via credit-only transactions.

### Dual-Sided Debt

When a transaction includes a negative credit (representing a debt obligation), it must also include a corresponding positive credit for the same asset in the same transaction. This ensures debt is always bilateral -- a debtor's obligation and a creditor's receivable are created atomically.

## Data Flow

```
                     TransactionBuilder
                     ├── .debit(tx_id, index, owner, amount)
                     ├── .credit(to, amount)
                     └── .build()
                            │
                            ▼
                     ┌──────────────┐
                     │  Validation  │
                     │  - signs     │
                     │  - balance   │
                     └──────┬───────┘
                            │ Transaction
                            ▼
                     ┌──────────────┐
                     │ Ledger.commit│
                     │  - token     │
                     │    lookup    │
                     │  - owner     │
                     │    match     │
                     │  - spend     │
                     │    check     │
                     └──────┬───────┘
                            │
                            ▼
                     ┌──────────────┐
                     │    Saga      │
                     │  mark_spent  │
                     │  insert_tok  │
                     │  insert_tx   │
                     └──────────────┘
```

### Two-Phase Validation

Validation is split across two layers to keep concerns separate:

1. **TransactionBuilder::build()** -- Structural validation that can be checked without storage access:
   - Per-asset conservation (debits == credits)
   - Dual-sided debt pairing

2. **Ledger::commit()** -- State-dependent validation that requires querying storage:
   - Token existence (debit references a real credit)
   - Token ownership match
   - Token asset/quantity match
   - Token spend status (not already spent)
   - Idempotency key uniqueness
   - Transaction ID verification

This separation means that a `Transaction` returned by `.build()` is structurally valid but not yet committed -- it must pass through `commit()` to become durable.

### Saga-Based Commit Pipeline

After validation, `Ledger::commit()` delegates to a three-step saga:

1. **Mark spent** — flag input tokens as consumed
2. **Create tokens** — insert new output tokens
3. **Insert transaction** — persist the transaction record

If any step fails, completed steps are compensated in reverse order. Each storage write method wraps its operations in a database transaction for per-step atomicity.

If compensation itself fails, a `CompensationFailed` error is returned (and logged) to signal the ledger may be inconsistent.

## Lock-Free Asset Registry

Assets are stored in an `Arc<ArcSwap<HashMap<String, Asset>>>`. The `ArcSwap` crate provides atomic pointer swaps, allowing reads to proceed without locks while writes (asset registration) atomically replace the map. This is optimal for the read-heavy, write-rare pattern of asset lookups.

## Deterministic Transaction IDs

Transaction IDs are derived deterministically from the transaction content:

```
tx_id = hex(SHA256(SHA256(canonical_preimage)))
```

The canonical preimage is a null-byte-delimited encoding of all debits, credits, and the idempotency key, preserving declaration order. Double SHA-256 guards against length-extension attacks.

This means:
- The same logical transaction always produces the same ID
- Tampering with any field changes the ID
- Callers can recompute the ID independently for verification

## Storage Abstraction

The `Storage` trait defines the persistence contract. All operations are async to support database-backed implementations. The trait requires `Send + Sync + Debug` for use behind `Arc<dyn Storage>`.

Write operations are granular primitives (`mark_spent`, `insert_credit_tokens`, `insert_tx`) composed by the saga layer into atomic commits with compensation on failure. Each write method should wrap its operations in a database transaction.

Two implementations are provided:
- `MemoryStorage` (in this crate) -- for testing and single-process use
- `SqliteStorage` (in `ledger-sqlite`) -- for persistent, concurrent use
