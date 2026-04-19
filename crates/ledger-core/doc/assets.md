# Assets & Quantities

## Asset Definition

An `Asset` has three properties:

| Property | Type | Description |
|----------|------|-------------|
| `name` | `String` | Unique identifier (e.g., `"usd"`, `"brush"`, `"gs"`) |
| `precision` | `u8` | Number of decimal places (0 for whole units) |
| `kind` | `AssetKind` | `Signed` (monetary) or `Unsigned` (physical) |

### AssetKind

```rust
pub enum AssetKind {
    Signed,    // Supports negative quantities (debt, payables)
    Unsigned,  // Quantities must be >= 0 (physical goods)
}
```

**Signed assets** are used for monetary values where negative quantities represent obligations. A credit of `-10.00 usd` to a debtor's account represents money they owe.

**Unsigned assets** are used for physical goods where negative quantities are nonsensical. You cannot have -5 brushes in inventory. The builder rejects negative quantities for unsigned assets at build time.

## Quantity Representation

Quantities are stored internally as `i128` integers scaled by `10^precision`. This eliminates floating-point precision issues entirely.

### Examples

| Asset | Precision | Display | Internal (i128) |
|-------|-----------|---------|------------------|
| `usd` | 2 | `"10.50"` | `1050` |
| `gs` | 0 | `"50000"` | `50000` |
| `brush` | 0 | `"5"` | `5` |
| `btc` | 8 | `"0.00100000"` | `100000` |

### `from_cents(qty: i128) -> String`

Converts a scaled integer back to its decimal string representation. Always includes trailing zeros up to the asset's precision.

```rust
let usd = Asset::new("usd", 2, AssetKind::Signed);
assert_eq!(usd.from_cents(1050), "10.50");
assert_eq!(usd.from_cents(100), "1.00");
assert_eq!(usd.from_cents(-500), "-5.00");
```

For precision 0, returns the integer directly:

```rust
let brush = Asset::new("brush", 0, AssetKind::Unsigned);
assert_eq!(brush.from_cents(5), "5");
```

### `parse_qty(s: &str) -> Result<i128, ParseQtyError>`

Parses a decimal string into a scaled integer. Validates that the string does not exceed the asset's precision.

```rust
let usd = Asset::new("usd", 2, AssetKind::Signed);
assert_eq!(usd.parse_qty("10.50")?, 1050);
assert_eq!(usd.parse_qty("10")?, 1000);    // Trailing zeros implied
assert_eq!(usd.parse_qty("-5.00")?, -500);
```

### ParseQtyError

| Variant | When |
|---------|------|
| `InvalidFormat(String)` | Not a valid decimal number |
| `TooManyDecimals { asset, max, got }` | More decimal places than asset precision |

## Asset Registration

Assets must be registered with the ledger before they can be used in transactions. Registration is idempotent -- registering the same asset twice is a no-op. However, registering an asset with the same name but different precision or kind returns `LedgerError::AssetConflict`.

```rust
ledger.register_asset(Asset::new("usd", 2, AssetKind::Signed)).await?;

// Idempotent: succeeds silently
ledger.register_asset(Asset::new("usd", 2, AssetKind::Signed)).await?;

// Conflict: same name, different precision
let err = ledger.register_asset(Asset::new("usd", 4, AssetKind::Signed)).await;
// => Err(LedgerError::AssetConflict { ... })
```

## Asset Lookup

The ledger maintains a lock-free cached copy of all registered assets via `ArcSwap`. This means asset lookups during transaction building are zero-copy and never block on storage.

```rust
// All assets as HashMap<String, Asset>
let all = ledger.assets();

// Single asset lookup
let usd: Option<Asset> = ledger.asset("usd");
```
