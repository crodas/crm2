# Error Handling

## LedgerError

All errors in `ledger-core` are represented by the `LedgerError` enum. Each variant corresponds to a specific validation failure or operational error.

### Token Errors

| Variant | When | Fields |
|---------|------|--------|
| `DebitNotFound(EntryRef)` | A debit references a token that does not exist in storage | The entry ref that was not found |
| `AlreadySpent(EntryRef)` | A debit references a token that has already been consumed | The entry ref of the spent token |

These errors occur during `Ledger::commit()` when debit references are resolved against storage.

### Asset Errors

| Variant | When | Fields |
|---------|------|--------|
| `UnknownAsset(String)` | A transaction references an asset that hasn't been registered | The unknown asset name |
| `AssetConflict { name, existing, incoming }` | Re-registering an asset with different precision or kind | Asset name, existing definition string, incoming definition string |
| `NegativeUnsigned { asset, qty }` | An unsigned asset has a negative quantity in a credit | Asset name, the negative quantity string |
| `InvalidQty(String)` | A quantity string failed to parse (not a valid decimal) | The invalid quantity string |

Asset errors occur during `TransactionBuilder::build()` except for `AssetConflict` which occurs during `register_asset()`.

### Conservation Errors

| Variant | When | Fields |
|---------|------|--------|
| `ConservationViolated { asset, debit_sum, credit_sum }` | Sum of debits != sum of credits for an asset | Asset name, debit total (i128), credit total (i128) |
| `DanglingDebt { asset }` | A negative credit exists without a matching positive credit | Asset name |

These errors occur during `TransactionBuilder::build()`. Conservation is only checked for assets that have debits (issuance is exempt).

### Account Errors

| Variant | When | Fields |
|---------|------|--------|
| `InvalidAccount(String)` | A storage-level issue with an account identifier | The invalid account string |

### Transaction Errors

| Variant | When | Fields |
|---------|------|--------|
| `TxIdMismatch { computed, stored }` | The computed tx_id doesn't match the one in the `Transaction` | Computed ID, stored ID |
| `DuplicateIdempotencyKey(String)` | A transaction with this key has already been committed | The duplicate key |

### Debit Mismatch Errors

| Variant | When | Fields |
|---------|------|--------|
| `DebitOwnerMismatch { entry_ref, expected, got }` | The token's owner doesn't match the debit's `owner` field | Entry ref, expected owner, actual owner |
| `DebitAssetMismatch { entry_ref, expected, got }` | The token's asset doesn't match the debit's `asset_name` field | Entry ref, expected asset, actual asset |
| `DebitQtyMismatch { entry_ref, expected, got }` | The token's quantity doesn't match the debit's `qty` field | Entry ref, expected qty, actual qty |

These three errors provide precise diagnostics when a debit references the right token but with wrong expectations. They occur during `Ledger::commit()`.

### Storage Errors

| Variant | When | Fields |
|---------|------|--------|
| `Storage(String)` | The storage backend encountered an error (I/O, database, etc.) | Error message from the backend |

This is the catch-all for backend-specific errors. Storage implementations convert their native errors (e.g., `sqlx::Error`) into this variant.

## Error Handling Patterns

### Building Transactions

```rust
match builder.build(&assets) {
    Ok(tx) => { /* proceed to commit */ }
    Err(LedgerError::UnknownAsset(name)) => {
        // Asset not registered -- register it first
    }
    Err(LedgerError::ConservationViolated { asset, debit_sum, credit_sum }) => {
        // Debits and credits don't balance
    }
    Err(e) => { /* other validation error */ }
}
```

### Committing Transactions

```rust
match ledger.commit(tx).await {
    Ok(tx_id) => { /* committed successfully */ }
    Err(LedgerError::AlreadySpent(eref)) => {
        // Token was spent between building and committing (race condition)
    }
    Err(LedgerError::DuplicateIdempotencyKey(key)) => {
        // Transaction already committed (safe retry detected)
    }
    Err(e) => { /* other commit error */ }
}
```

### Error Propagation

All error variants implement `std::fmt::Display` and `std::error::Error`, making them compatible with `?` propagation and error-reporting crates like `anyhow`.
