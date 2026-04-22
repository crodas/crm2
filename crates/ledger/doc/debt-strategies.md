# Debt Strategies

## DebtStrategy Trait

The `DebtStrategy` trait defines a pluggable interface for modeling debt in the ledger. Strategies are configured on the `Ledger` and automatically passed to each `TransactionBuilder`, which exposes `create_debt()` and `settle_debt()` as part of its fluent API.

The trait has two methods:

```rust
#[async_trait]
pub trait DebtStrategy: Send + Sync {
    fn issue(
        &self,
        builder: TransactionBuilder,
        entity_id: &str,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error>;

    async fn settle(
        &self,
        builder: TransactionBuilder,
        entity_id: &str,
        asset: &Asset,
        amount: i128,
    ) -> Result<TransactionBuilder, Error>;
}
```

Strategies are constructed with debtor/creditor path templates containing `{id}`, which is replaced with the entity identifier at call time.

### Design Philosophy

The core ledger enforces only the conservation law (debits == credits per asset) and the dual-sided debt rule (negative credits must pair with positive credits). It has no concept of "debt" as a domain object.

Strategies decide:
- What assets to use (same asset vs. separate `.d` asset)
- What signs to use (positive/negative credits vs. explicit debt tokens)
- How to select tokens for settlement (greedy, FIFO, etc.)
- Whether settlement consumes tokens or creates offsetting entries

As long as the resulting transaction satisfies the core conservation invariant, the ledger accepts it.

### Why `issue` is Sync and `settle` is Async

Issuing debt only adds credits to the builder -- no storage queries needed. Settlement, however, may need to query unspent tokens (e.g., `SplitAssetDebt` must find and select debt tokens to consume). Hence `settle` is async.

### Path Templates

Each strategy is constructed with debtor/creditor path templates:

```rust
let strategy = SignedPositionDebt::new(
    "customer/{id}/debt",        // debtor template
    "store/receivables/{id}",    // creditor template
);
```

The `{id}` placeholder is replaced with the entity identifier when `create_debt(entity_id, ...)` or `settle_debt(entity_id, ...)` is called. `entity_id` is `&str`. `resolve_template` returns a plain `String` (not `Result`). This keeps account path conventions in one place (strategy configuration) rather than scattered across route handlers.

## SignedPositionDebt

### How It Works

Uses positive and negative credits on the **same asset** to represent debt positions:

| Operation | Debtor Account | Creditor Account |
|-----------|----------------|------------------|
| Issue debt | Credit `-amount` | Credit `+amount` |
| Settle debt | Credit `+amount` | Credit `-amount` |

Settlement creates **offsetting** credits rather than consuming prior tokens. The net balance converges to zero as debt is settled.

### Example: Issue and Settle

```rust
// Issue: customer owes 50.00 usd
// Creates:
//   customer/debt:  -5000 (owes money)
//   store/receivable: +5000 (is owed money)

// After issue:
//   customer/debt balance: -5000
//   store/receivable balance: +5000

// Settle: customer pays 50.00 usd
// Creates:
//   customer/debt:  +5000 (offset)
//   store/receivable: -5000 (offset)

// After settle:
//   customer/debt balance: 0  (net of -5000 + 5000)
//   store/receivable balance: 0  (net of +5000 - 5000)
```

### Advantages

| Advantage | Detail |
|-----------|--------|
| **Simplicity** | No extra asset registration. No token selection for settlement. |
| **Single balance query** | Net position is directly readable: `balance("customer/debt", "usd")` |
| **Mixed transactions** | Debt entries mix naturally with product debits in the same transaction |
| **No UTXO fragmentation** | Settlement creates new tokens instead of splitting existing ones |

### Disadvantages

| Disadvantage | Detail |
|--------------|--------|
| **Unbounded tokens** | Tokens are never consumed -- they accumulate forever |
| **No double-spend protection** | The same debt can be "settled" multiple times (balance goes positive) |
| **Ambiguity** | Negative balance could mean debt or an accounting error -- no structural distinction |
| **Audit complexity** | Tracing the lifecycle of a specific debt requires scanning all tokens |

### When to Use

Best for low-volume systems where:
- Debts are settled in full and promptly
- Token accumulation is not a concern
- Simplicity is more valuable than auditability

## SplitAssetDebt

### How It Works

Creates a **separate debt asset** named `{base_asset}.d` (e.g., `usd.d`). Debt tokens on this asset are consumed via UTXO debits during settlement.

| Operation | Debtor Account | Creditor Account |
|-----------|----------------|------------------|
| Issue debt | Credit `-amount` on `{asset}.d` | Credit `+amount` on `{asset}.d` |
| Settle debt | Debit negative tokens from debtor | Debit positive tokens from creditor |

### Example: Issue and Settle

```rust
// Register debt asset
SplitAssetDebt::register_debt_asset(&ledger, &usd_asset).await?;
// Creates: Asset { name: "usd.d", precision: 2, kind: Signed }

// Issue: customer owes 50.00
// Creates tokens on usd.d:
//   customer/debt: -5000  (token A)
//   store/receivable: +5000  (token B)

// Settle: customer pays 50.00
// Debits:
//   Consume token A (customer/debt, -5000)
//   Consume token B (store/receivable, +5000)
// Sum of debits: -5000 + 5000 = 0
// Sum of credits: 0
// Conservation holds: 0 == 0
```

### Partial Settlement

```rust
// Issue: customer owes 100.00
// Creates:
//   customer/debt: -10000  (token A)
//   store/receivable: +10000  (token B)

// Settle 60.00:
// Debits:
//   Consume token A (-10000)
//   Consume token B (+10000)
// Credits (change):
//   customer/debt: -4000  (remaining debt)
//   store/receivable: +4000  (remaining receivable)
```

### Token Selection Algorithm

**Debtor side** (negative tokens):
1. Query unspent tokens for debtor account on `{asset}.d`
2. Filter for negative quantities only
3. Sort ascending (most negative first)
4. Accumulate until `abs(sum) >= amount`

**Creditor side** (positive tokens):
1. Query unspent tokens for creditor account on `{asset}.d`
2. Filter for positive quantities only
3. Sort descending (largest first)
4. Accumulate until `sum >= amount`

Both sides generate change credits if the selected tokens exceed the settlement amount.

### Query Helpers

```rust
// How much does entity "customer-1" owe? (absolute value of negative balance)
let owed = strategy.owed_by(&ledger, "customer-1", &usd_asset).await?;

// How much is owed to entity "customer-1"? (positive balance)
let receivable = strategy.owed_to(&ledger, "customer-1", &usd_asset).await?;
```

### Advantages

| Advantage | Detail |
|-----------|--------|
| **Clean separation** | Money (`usd`) and obligations (`usd.d`) are structurally different |
| **Double-spend protection** | Debt tokens are consumed -- you can't settle the same debt twice |
| **Bounded token count** | Settled tokens are spent, not accumulated |
| **Auditability** | UTXO chain traces the full lifecycle of each debt |
| **Query helpers** | `owed_by()` and `owed_to()` provide domain-specific queries |

### Disadvantages

| Disadvantage | Detail |
|--------------|--------|
| **Extra asset** | Must register `{base}.d` asset before issuing debt |
| **UTXO fragmentation** | Partial payments create change tokens on both sides |
| **Storage coupling** | Needs `Arc<dyn Storage>` at construction (for token queries during settlement) |
| **Complexity** | Token selection for both sides adds implementation and cognitive overhead |

### When to Use

Best for systems where:
- Debts may be partially settled over multiple transactions
- Audit trail and double-spend protection matter
- Token count should be bounded (high-volume systems)
- You need explicit debt lifecycle tracking

## Comparison Matrix

| Aspect | SignedPositionDebt | SplitAssetDebt |
|--------|--------------------|----------------|
| Asset model | Same asset | Separate `.d` asset |
| Issue mechanism | Credits only | Credits only |
| Settle mechanism | Offsetting credits | UTXO debits + change |
| Token lifecycle | Accumulate forever | Consumed on settlement |
| Double-spend protection | No | Yes |
| Partial settlement | Trivial (just credit less) | Token selection + change |
| Storage needed at construction | No | Yes |
| Extra asset registration | No | Yes |
| Net balance query | Direct (`balance()`) | Via `owed_by()` / `owed_to()` |
| Best for | Simple, low-volume | Complex, high-volume |
