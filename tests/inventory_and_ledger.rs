use crm2::version;
use sqlx::SqlitePool;

/// Helper: init an in-memory database with all migrations applied.
async fn test_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    crm2::db::init_pool_with(&pool).await.unwrap();
    pool
}

/// Insert the minimal reference data needed by most tests.
/// Note: customer_types and customer_groups are already seeded by migrations.
async fn seed_base(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO customers (id, customer_type_id, name) VALUES (1, 1, 'Test Customer')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products (id, sku, name, unit) VALUES (1, 'CEM-001', 'Cement', 'bag')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO warehouses (id, name) VALUES (1, 'Main')")
        .execute(pool)
        .await
        .unwrap();
}

/// Helper: receive inventory via direct SQL (mirrors the route handler logic).
async fn receive_stock(
    pool: &SqlitePool,
    product_id: i64,
    warehouse_id: i64,
    qty: f64,
    cost_cents: i64,
) -> i64 {
    let total_cost = (qty * cost_cents as f64).round() as i64;
    let prev_rcpt = version::latest_version_id(pool, "inventory_receipts")
        .await
        .unwrap();
    let rcpt_vid = version::compute_version_id(
        &version::inventory_receipt_fields(&Some("test".to_string()), &None, &None, total_cost),
        &prev_rcpt,
    );

    let receipt_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_receipts (reference, total_cost, version_id) VALUES ('test', ?, ?) RETURNING id",
    )
    .bind(total_cost)
    .bind(&rcpt_vid)
    .fetch_one(pool)
    .await
    .unwrap();

    let prev_utxo = version::latest_version_id(pool, "inventory_utxos")
        .await
        .unwrap();
    let utxo_vid = version::compute_version_id(
        &version::inventory_utxo_fields(
            product_id,
            warehouse_id,
            qty,
            cost_cents,
            Some(receipt_id),
            None,
        ),
        &prev_utxo,
    );

    sqlx::query(
        "INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id, version_id)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(product_id)
    .bind(warehouse_id)
    .bind(qty)
    .bind(cost_cents)
    .bind(receipt_id)
    .bind(&utxo_vid)
    .execute(pool)
    .await
    .unwrap();

    receipt_id
}

/// Helper: get total unspent stock for a product+warehouse.
async fn unspent_stock(pool: &SqlitePool, product_id: i64, warehouse_id: i64) -> f64 {
    sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(SUM(quantity), 0.0) FROM inventory_utxos
         WHERE product_id = ? AND warehouse_id = ? AND spent = 0",
    )
    .bind(product_id)
    .bind(warehouse_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Helper: count unspent UTXOs for a product+warehouse.
async fn unspent_utxo_count(pool: &SqlitePool, product_id: i64, warehouse_id: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM inventory_utxos
         WHERE product_id = ? AND warehouse_id = ? AND spent = 0",
    )
    .bind(product_id)
    .bind(warehouse_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ─── UTXO / Stock invariant tests ────────────────────────────────

#[tokio::test]
async fn receive_inventory_creates_unspent_utxo() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 100.0, 4500000).await;

    assert_eq!(unspent_stock(&pool, 1, 1).await, 100.0);
    assert_eq!(unspent_utxo_count(&pool, 1, 1).await, 1);
}

#[tokio::test]
async fn receive_multiple_lots_sums_correctly() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 50.0, 4500000).await;
    receive_stock(&pool, 1, 1, 30.0, 4600000).await;

    assert_eq!(unspent_stock(&pool, 1, 1).await, 80.0);
    assert_eq!(unspent_utxo_count(&pool, 1, 1).await, 2);
}

#[tokio::test]
async fn sale_fully_consumes_single_utxo() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 10.0, 4500000).await;

    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 10.0, 6300000)])
        .await
        .unwrap();

    // All stock consumed — zero unspent
    assert_eq!(unspent_stock(&pool, 1, 1).await, 0.0);
    assert_eq!(unspent_utxo_count(&pool, 1, 1).await, 0);
}

#[tokio::test]
async fn sale_partial_consumption_creates_change_utxo() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 100.0, 4500000).await;

    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 30.0, 6300000)])
        .await
        .unwrap();

    // 100 - 30 = 70 remaining in a change UTXO
    assert_eq!(unspent_stock(&pool, 1, 1).await, 70.0);
    // Original is spent, change UTXO is unspent
    assert_eq!(unspent_utxo_count(&pool, 1, 1).await, 1);

    // The change UTXO must inherit cost_per_unit from the original
    let cost: i64 = sqlx::query_scalar(
        "SELECT cost_per_unit FROM inventory_utxos WHERE spent = 0 AND product_id = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cost, 4500000);
}

#[tokio::test]
async fn sale_fifo_consumes_oldest_utxo_first() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    // Two lots with different costs — first is older (FIFO)
    receive_stock(&pool, 1, 1, 20.0, 4000000).await; // older, cheaper
    receive_stock(&pool, 1, 1, 20.0, 5000000).await; // newer, pricier

    // Sell 25: should consume all of first lot (20) + 5 from second
    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 25.0, 6000000)])
        .await
        .unwrap();

    assert_eq!(unspent_stock(&pool, 1, 1).await, 15.0);

    // Remaining UTXO should have the newer cost (from second lot's change)
    let cost: i64 = sqlx::query_scalar(
        "SELECT cost_per_unit FROM inventory_utxos WHERE spent = 0 AND product_id = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cost, 5000000);
}

#[tokio::test]
async fn sale_spanning_three_utxos() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 10.0, 1000).await;
    receive_stock(&pool, 1, 1, 10.0, 2000).await;
    receive_stock(&pool, 1, 1, 10.0, 3000).await;

    // Sell 25: consumes first two fully + 5 from third
    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 25.0, 5000)])
        .await
        .unwrap();

    assert_eq!(unspent_stock(&pool, 1, 1).await, 5.0);
    assert_eq!(unspent_utxo_count(&pool, 1, 1).await, 1);
}

#[tokio::test]
async fn sale_insufficient_stock_is_rejected() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 5.0, 4500000).await;

    let result =
        crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 10.0, 6300000)]).await;
    assert!(result.is_err());

    // Stock must be untouched — transaction rolled back
    assert_eq!(unspent_stock(&pool, 1, 1).await, 5.0);
    assert_eq!(unspent_utxo_count(&pool, 1, 1).await, 1);
}

#[tokio::test]
async fn sale_zero_stock_is_rejected() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let result =
        crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 1.0, 6300000)]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn sale_records_utxo_audit_trail() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 50.0, 1000).await;

    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 30.0, 2000)])
        .await
        .unwrap();

    // sale_line_utxo_inputs must record the consumption
    let inputs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sale_line_utxo_inputs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(inputs, 1);

    let qty_used: f64 =
        sqlx::query_scalar("SELECT quantity_used FROM sale_line_utxo_inputs LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(qty_used, 30.0);
}

#[tokio::test]
async fn sale_total_equals_sum_of_lines() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    // Two products
    sqlx::query(
        "INSERT INTO products (id, sku, name, unit) VALUES (2, 'BRK-001', 'Brick', 'unit')",
    )
    .execute(&pool)
    .await
    .unwrap();
    receive_stock(&pool, 1, 1, 100.0, 1000).await;
    receive_stock(&pool, 2, 1, 200.0, 500).await;

    // Sell: 10 cement @ 20.00 + 50 bricks @ 8.00
    let sale = crm2::routes::sales::create_sale_tx(
        &pool,
        1,
        1,
        None,
        &[(1, 1, 10.0, 2000), (2, 1, 50.0, 800)],
    )
    .await
    .unwrap();

    // total = 10*20.00 + 50*8.00 = 200+400 = 600.00 = 60000 cents
    assert_eq!(sale.total_amount.cents(), 60000);
}

#[tokio::test]
async fn spent_utxo_not_double_spent() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 10.0, 1000).await;

    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 10.0, 2000)])
        .await
        .unwrap();

    // Second sale with no stock left must fail
    let result = crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 1.0, 2000)]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn change_utxo_is_spendable() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 100.0, 1000).await;

    // First sale: partial, creates change
    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 60.0, 2000)])
        .await
        .unwrap();
    assert_eq!(unspent_stock(&pool, 1, 1).await, 40.0);

    // Second sale: uses the change UTXO
    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 25.0, 2000)])
        .await
        .unwrap();
    assert_eq!(unspent_stock(&pool, 1, 1).await, 15.0);

    // Third sale: uses the change of the change
    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 15.0, 2000)])
        .await
        .unwrap();
    assert_eq!(unspent_stock(&pool, 1, 1).await, 0.0);
}

// ─── Payment ledger tests ────────────────────────────────────────

/// Helper: create a quote with status 'accepted' and given total (in cents).
async fn create_accepted_quote(pool: &SqlitePool, customer_id: i64, total_cents: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO quotes (customer_id, status, title, total_amount, is_debt)
         VALUES (?, 'accepted', 'Test Quote', ?, 0) RETURNING id",
    )
    .bind(customer_id)
    .bind(total_cents)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Helper: record a payment against a quote (in cents).
async fn pay(pool: &SqlitePool, quote_id: i64, amount_cents: i64) {
    let prev = version::latest_version_id(pool, "payment_utxos")
        .await
        .unwrap();
    let vid = version::compute_version_id(
        &version::payment_utxo_fields(quote_id, amount_cents, &Some("cash".to_string()), &None),
        &prev,
    );

    sqlx::query(
        "INSERT INTO payment_utxos (quote_id, amount, method, version_id) VALUES (?, ?, 'cash', ?)",
    )
    .bind(quote_id)
    .bind(amount_cents)
    .bind(&vid)
    .execute(pool)
    .await
    .unwrap();
}

/// Helper: compute balance via the same query the route handler uses.
async fn customer_balance(pool: &SqlitePool, customer_id: i64) -> (i64, i64, i64) {
    let row = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            COALESCE(SUM(q.total_amount), 0),
            COALESCE(SUM(COALESCE(p.paid, 0)), 0)
         FROM quotes q
         LEFT JOIN (
            SELECT quote_id, SUM(amount) as paid
            FROM payment_utxos
            GROUP BY quote_id
         ) p ON p.quote_id = q.id
         WHERE q.customer_id = ? AND q.status IN ('accepted', 'booked')",
    )
    .bind(customer_id)
    .fetch_one(pool)
    .await
    .unwrap();
    (row.0, row.1, row.0 - row.1)
}

#[tokio::test]
async fn payment_reduces_outstanding_balance() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 100_000).await;

    let (owed, paid, outstanding) = customer_balance(&pool, 1).await;
    assert_eq!(owed, 100_000);
    assert_eq!(paid, 0);
    assert_eq!(outstanding, 100_000);

    pay(&pool, q, 30_000).await;
    let (_, paid, outstanding) = customer_balance(&pool, 1).await;
    assert_eq!(paid, 30_000);
    assert_eq!(outstanding, 70_000);
}

#[tokio::test]
async fn multiple_payments_accumulate() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 100_000).await;
    pay(&pool, q, 20_000).await;
    pay(&pool, q, 30_000).await;
    pay(&pool, q, 10_000).await;

    let (_, paid, outstanding) = customer_balance(&pool, 1).await;
    assert_eq!(paid, 60_000);
    assert_eq!(outstanding, 40_000);
}

#[tokio::test]
async fn overpayment_results_in_negative_outstanding() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 50_000).await;
    pay(&pool, q, 80_000).await;

    let (owed, paid, outstanding) = customer_balance(&pool, 1).await;
    assert_eq!(owed, 50_000);
    assert_eq!(paid, 80_000);
    assert_eq!(outstanding, -30_000); // credit
}

#[tokio::test]
async fn balance_aggregates_multiple_quotes() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q1 = create_accepted_quote(&pool, 1, 100_000).await;
    let q2 = create_accepted_quote(&pool, 1, 200_000).await;

    pay(&pool, q1, 50_000).await;
    pay(&pool, q2, 100_000).await;

    let (owed, paid, outstanding) = customer_balance(&pool, 1).await;
    assert_eq!(owed, 300_000);
    assert_eq!(paid, 150_000);
    assert_eq!(outstanding, 150_000);
}

#[tokio::test]
async fn draft_quotes_excluded_from_balance() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    // Draft quote — should not count
    sqlx::query(
        "INSERT INTO quotes (customer_id, status, title, total_amount) VALUES (1, 'draft', 'Draft', 999_999)",
    )
    .execute(&pool)
    .await
    .unwrap();

    create_accepted_quote(&pool, 1, 50_000).await;

    let (owed, _, _) = customer_balance(&pool, 1).await;
    assert_eq!(owed, 50_000); // only the accepted quote
}

#[tokio::test]
async fn payment_ledger_is_append_only() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 100_000).await;
    pay(&pool, q, 10_000).await;
    pay(&pool, q, 20_000).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payment_utxos WHERE quote_id = ?")
        .bind(q)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);

    // Each entry is individually preserved
    let amounts: Vec<i64> =
        sqlx::query_scalar("SELECT amount FROM payment_utxos WHERE quote_id = ? ORDER BY id")
            .bind(q)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(amounts, vec![10_000, 20_000]);
}

#[tokio::test]
async fn debt_creates_accepted_quote_with_is_debt() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    sqlx::query(
        "INSERT INTO quotes (customer_id, status, title, total_amount, is_debt)
         VALUES (1, 'accepted', 'Debt entry', 75_000, 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Debts (is_debt=1) with status 'accepted' count towards balance
    let (owed, _, outstanding) = customer_balance(&pool, 1).await;
    assert_eq!(owed, 75_000);
    assert_eq!(outstanding, 75_000);
}

// ─── Database constraint tests ───────────────────────────────────

#[tokio::test]
async fn utxo_quantity_must_be_positive() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let result = sqlx::query(
        "INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit)
         VALUES (1, 1, 0, 1000)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err()); // CHECK (quantity > 0)
}

#[tokio::test]
async fn utxo_negative_quantity_rejected() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let result = sqlx::query(
        "INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit)
         VALUES (1, 1, -5, 1000)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn payment_amount_must_be_positive() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 100_000).await;

    let result =
        sqlx::query("INSERT INTO payment_utxos (quote_id, amount, method) VALUES (?, 0, 'cash')")
            .bind(q)
            .execute(&pool)
            .await;

    assert!(result.is_err()); // CHECK (amount > 0)
}

#[tokio::test]
async fn cost_per_unit_must_be_non_negative() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let result = sqlx::query(
        "INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit)
         VALUES (1, 1, 10, -100)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err()); // CHECK (cost_per_unit >= 0)
}

#[tokio::test]
async fn quote_status_check_constraint() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let result = sqlx::query(
        "INSERT INTO quotes (customer_id, status, title, total_amount) VALUES (1, 'invalid', 'Bad', 0)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err()); // CHECK status IN (...)
}

#[tokio::test]
async fn foreign_key_prevents_orphan_utxo() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let result = sqlx::query(
        "INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit)
         VALUES (999, 1, 10, 1000)",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err()); // FK constraint: product 999 doesn't exist
}

// ─── Version chain tests ────────────────────────────────────────

#[tokio::test]
async fn sale_creates_version_chain_for_all_tables() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    receive_stock(&pool, 1, 1, 100.0, 4500000).await;

    // Sale triggers inserts in: sales, sale_lines, sale_line_utxo_inputs, inventory_utxos (change)
    crm2::routes::sales::create_sale_tx(&pool, 1, 1, None, &[(1, 1, 30.0, 6300000)])
        .await
        .unwrap();

    // All version_ids created by create_sale_tx must be non-empty sha256 hex (64 chars)
    let sale_vid: String =
        sqlx::query_scalar("SELECT version_id FROM sales ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sale_vid.len(), 64, "sale version_id should be sha256 hex");

    let sl_vid: String =
        sqlx::query_scalar("SELECT version_id FROM sale_lines ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sl_vid.len(), 64);

    let input_vid: String =
        sqlx::query_scalar("SELECT version_id FROM sale_line_utxo_inputs ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(input_vid.len(), 64);

    // Change UTXO was created (100 - 30 = 70)
    let change_vid: String = sqlx::query_scalar(
        "SELECT version_id FROM inventory_utxos WHERE source_sale_id IS NOT NULL ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(change_vid.len(), 64);
}

#[tokio::test]
async fn version_chain_links_to_previous() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    // Two receipts -> two inventory_receipts rows with a chain
    // The backfill from migration computes chain for rows inserted with empty version_id.
    // But receive_stock uses direct SQL, so they get backfilled version_ids.
    receive_stock(&pool, 1, 1, 50.0, 1000).await;
    receive_stock(&pool, 1, 1, 30.0, 2000).await;

    let vids: Vec<String> =
        sqlx::query_scalar("SELECT version_id FROM inventory_receipts ORDER BY id ASC")
            .fetch_all(&pool)
            .await
            .unwrap();

    // Both must be valid hashes (backfilled by recompute_all_chains)
    assert_eq!(vids.len(), 2);
    assert_eq!(vids[0].len(), 64);
    assert_eq!(vids[1].len(), 64);
    // They must differ (different content + chained)
    assert_ne!(vids[0], vids[1]);

    // Verify chain: recompute second hash using first as prev
    // second receipt: qty=30, cost=2000 -> total_cost = 60000
    let fields =
        crm2::version::inventory_receipt_fields(&Some("test".to_string()), &None, &None, 60000);
    let expected = crm2::version::compute_version_id(&fields, &vids[0]);
    assert_eq!(vids[1], expected, "second version_id must chain from first");
}

#[tokio::test]
async fn payment_utxo_gets_version_id() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 100_000).await;
    pay(&pool, q, 30_000).await;
    pay(&pool, q, 20_000).await;

    // Payments inserted via direct SQL get backfilled version_ids
    let vids: Vec<String> =
        sqlx::query_scalar("SELECT version_id FROM payment_utxos ORDER BY id ASC")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(vids.len(), 2);
    assert_eq!(vids[0].len(), 64);
    assert_eq!(vids[1].len(), 64);
    assert_ne!(vids[0], vids[1]);
}

// ── Receivables Tests ──────────────────────────────────────────────

/// Helper: compute total receivables (same query as the route handler).
async fn total_receivables(pool: &SqlitePool) -> (i64, i64, i64) {
    let row = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            COALESCE(SUM(q.total_amount), 0),
            COALESCE(SUM(COALESCE(p.paid, 0)), 0)
         FROM quotes q
         LEFT JOIN (
            SELECT quote_id, SUM(amount) as paid
            FROM payment_utxos
            GROUP BY quote_id
         ) p ON p.quote_id = q.id
         WHERE q.status IN ('accepted', 'booked')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    (row.0, row.1, row.0 - row.1)
}

#[tokio::test]
async fn receivables_starts_at_zero() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let (owed, paid, outstanding) = total_receivables(&pool).await;
    assert_eq!(owed, 0);
    assert_eq!(paid, 0);
    assert_eq!(outstanding, 0);
}

#[tokio::test]
async fn receivables_reflects_accepted_quotes() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    create_accepted_quote(&pool, 1, 50000).await;
    create_accepted_quote(&pool, 1, 30000).await;

    let (owed, paid, outstanding) = total_receivables(&pool).await;
    assert_eq!(owed, 80000);
    assert_eq!(paid, 0);
    assert_eq!(outstanding, 80000);
}

#[tokio::test]
async fn receivables_decreases_with_payment() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 100000).await;

    pay(&pool, q, 40000).await;
    let (owed, paid, outstanding) = total_receivables(&pool).await;
    assert_eq!(owed, 100000);
    assert_eq!(paid, 40000);
    assert_eq!(outstanding, 60000);

    pay(&pool, q, 60000).await;
    let (_, _, outstanding) = total_receivables(&pool).await;
    assert_eq!(outstanding, 0);
}

#[tokio::test]
async fn receivables_aggregates_multiple_customers() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    // Add a second customer
    sqlx::query("INSERT INTO customers (id, customer_type_id, name) VALUES (2, 1, 'Customer 2')")
        .execute(&pool)
        .await
        .unwrap();

    let q1 = create_accepted_quote(&pool, 1, 50000).await;
    let q2 = create_accepted_quote(&pool, 2, 70000).await;

    let (owed, _, outstanding) = total_receivables(&pool).await;
    assert_eq!(owed, 120000);
    assert_eq!(outstanding, 120000);

    pay(&pool, q1, 50000).await;
    let (_, paid, outstanding) = total_receivables(&pool).await;
    assert_eq!(paid, 50000);
    assert_eq!(outstanding, 70000);

    pay(&pool, q2, 70000).await;
    let (_, _, outstanding) = total_receivables(&pool).await;
    assert_eq!(outstanding, 0);
}

#[tokio::test]
async fn receivables_excludes_draft_quotes() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    // Draft quote should not count
    sqlx::query(
        "INSERT INTO quotes (customer_id, status, title, total_amount, is_debt) VALUES (1, 'draft', 'Draft', 99999, 0)",
    ).execute(&pool).await.unwrap();

    create_accepted_quote(&pool, 1, 20000).await;

    let (owed, _, outstanding) = total_receivables(&pool).await;
    assert_eq!(owed, 20000, "draft quote should be excluded");
    assert_eq!(outstanding, 20000);
}

#[tokio::test]
async fn receivables_updates_after_each_payment() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let q = create_accepted_quote(&pool, 1, 100000).await;

    // Track outstanding after each payment
    let payments = vec![10000, 25000, 15000, 50000];
    let mut total_paid_so_far = 0_i64;

    for amount in payments {
        pay(&pool, q, amount).await;
        total_paid_so_far += amount;
        let (_, paid, outstanding) = total_receivables(&pool).await;
        assert_eq!(paid, total_paid_so_far, "total_paid should accumulate");
        assert_eq!(
            outstanding,
            100000 - total_paid_so_far,
            "outstanding should decrease with each payment"
        );
    }
}

// ── Supplier Ledger Tests ──────────────────────────────────────────

/// Helper: create a receipt with a debt entry (credit purchase).
async fn receive_on_credit(
    pool: &SqlitePool,
    product_id: i64,
    warehouse_id: i64,
    qty: f64,
    cost_cents: i64,
) -> i64 {
    let receipt_id = receive_stock(pool, product_id, warehouse_id, qty, cost_cents).await;
    let total_cost = (qty * cost_cents as f64).round() as i64;

    // Create debt entry (negative)
    let prev = version::latest_version_id(pool, "supplier_ledger_utxos")
        .await
        .unwrap();
    let vid = version::compute_version_id(
        &version::supplier_ledger_utxo_fields(
            receipt_id,
            -total_cost,
            &None,
            &Some("Inventory received".to_string()),
        ),
        &prev,
    );
    sqlx::query(
        "INSERT INTO supplier_ledger_utxos (receipt_id, amount, notes, version_id) VALUES (?, ?, 'Inventory received', ?)",
    )
    .bind(receipt_id)
    .bind(-total_cost)
    .bind(&vid)
    .execute(pool)
    .await
    .unwrap();

    receipt_id
}

/// Helper: record a supplier payment (positive entry).
async fn supplier_pay(pool: &SqlitePool, receipt_id: i64, amount_cents: i64) {
    let prev = version::latest_version_id(pool, "supplier_ledger_utxos")
        .await
        .unwrap();
    let method = Some("cash".to_string());
    let vid = version::compute_version_id(
        &version::supplier_ledger_utxo_fields(receipt_id, amount_cents, &method, &None),
        &prev,
    );
    sqlx::query(
        "INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, version_id) VALUES (?, ?, 'cash', ?)",
    )
    .bind(receipt_id)
    .bind(amount_cents)
    .bind(&vid)
    .execute(pool)
    .await
    .unwrap();
}

/// Helper: get supplier balance from the ledger.
async fn supplier_balance(pool: &SqlitePool) -> (i64, i64, i64) {
    let row = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            COALESCE(SUM(CASE WHEN amount < 0 THEN ABS(amount) ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN amount > 0 THEN amount ELSE 0 END), 0)
         FROM supplier_ledger_utxos",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let (total_owed, total_paid) = row;
    (total_owed, total_paid, total_owed - total_paid)
}

/// Helper: get supplier balance for a specific receipt.
async fn receipt_balance(pool: &SqlitePool, receipt_id: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(amount), 0) FROM supplier_ledger_utxos WHERE receipt_id = ?",
    )
    .bind(receipt_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn credit_receipt_creates_debt() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let rid = receive_on_credit(&pool, 1, 1, 10.0, 5000).await;

    // total_cost = 10 * 5000 = 50000 cents
    let balance = receipt_balance(&pool, rid).await;
    assert_eq!(
        balance, -50000,
        "credit receipt should create negative balance"
    );

    let (owed, paid, outstanding) = supplier_balance(&pool).await;
    assert_eq!(owed, 50000);
    assert_eq!(paid, 0);
    assert_eq!(outstanding, 50000);
}

#[tokio::test]
async fn payment_reduces_supplier_debt() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let rid = receive_on_credit(&pool, 1, 1, 10.0, 5000).await;
    // Debt = 50000

    supplier_pay(&pool, rid, 20000).await;

    let balance = receipt_balance(&pool, rid).await;
    assert_eq!(balance, -30000, "payment should reduce debt");

    let (owed, paid, outstanding) = supplier_balance(&pool).await;
    assert_eq!(owed, 50000);
    assert_eq!(paid, 20000);
    assert_eq!(outstanding, 30000);
}

#[tokio::test]
async fn multiple_payments_accumulate_for_supplier() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let rid = receive_on_credit(&pool, 1, 1, 10.0, 5000).await;

    supplier_pay(&pool, rid, 10000).await;
    supplier_pay(&pool, rid, 15000).await;
    supplier_pay(&pool, rid, 25000).await;

    let balance = receipt_balance(&pool, rid).await;
    assert_eq!(balance, 0, "debt should be fully paid off");

    let (owed, paid, outstanding) = supplier_balance(&pool).await;
    assert_eq!(owed, 50000);
    assert_eq!(paid, 50000);
    assert_eq!(outstanding, 0);
}

#[tokio::test]
async fn paid_cash_creates_zero_balance() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let rid = receive_stock(&pool, 1, 1, 10.0, 5000).await;
    let total_cost = 50000_i64;

    // Debt entry
    let prev1 = version::latest_version_id(&pool, "supplier_ledger_utxos")
        .await
        .unwrap();
    let vid1 = version::compute_version_id(
        &version::supplier_ledger_utxo_fields(rid, -total_cost, &None, &None),
        &prev1,
    );
    sqlx::query(
        "INSERT INTO supplier_ledger_utxos (receipt_id, amount, version_id) VALUES (?, ?, ?)",
    )
    .bind(rid)
    .bind(-total_cost)
    .bind(&vid1)
    .execute(&pool)
    .await
    .unwrap();

    // Immediate cash payment
    let prev2 = version::latest_version_id(&pool, "supplier_ledger_utxos")
        .await
        .unwrap();
    let method = Some("cash".to_string());
    let vid2 = version::compute_version_id(
        &version::supplier_ledger_utxo_fields(rid, total_cost, &method, &None),
        &prev2,
    );
    sqlx::query("INSERT INTO supplier_ledger_utxos (receipt_id, amount, method, version_id) VALUES (?, ?, 'cash', ?)")
        .bind(rid).bind(total_cost).bind(&vid2).execute(&pool).await.unwrap();

    let balance = receipt_balance(&pool, rid).await;
    assert_eq!(balance, 0, "paid cash should net to zero");
}

#[tokio::test]
async fn no_overpayment_beyond_debt() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let rid = receive_on_credit(&pool, 1, 1, 5.0, 2000).await;
    // Debt = 10000

    supplier_pay(&pool, rid, 10000).await;
    // Fully paid

    // Overpayment - balance goes positive (credit to the business)
    supplier_pay(&pool, rid, 5000).await;

    let balance = receipt_balance(&pool, rid).await;
    assert_eq!(
        balance, 5000,
        "overpayment should result in positive balance (credit)"
    );

    let (owed, paid, outstanding) = supplier_balance(&pool).await;
    assert_eq!(owed, 10000);
    assert_eq!(paid, 15000);
    // outstanding is negative (supplier owes us)
    assert_eq!(outstanding, -5000);
}

#[tokio::test]
async fn multiple_receipts_aggregate_supplier_debt() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let r1 = receive_on_credit(&pool, 1, 1, 10.0, 5000).await; // debt 50000
    let r2 = receive_on_credit(&pool, 1, 1, 20.0, 3000).await; // debt 60000

    let (owed, paid, outstanding) = supplier_balance(&pool).await;
    assert_eq!(owed, 110000);
    assert_eq!(paid, 0);
    assert_eq!(outstanding, 110000);

    supplier_pay(&pool, r1, 50000).await; // pay off r1 completely

    let (owed, paid, outstanding) = supplier_balance(&pool).await;
    assert_eq!(owed, 110000);
    assert_eq!(paid, 50000);
    assert_eq!(outstanding, 60000);

    supplier_pay(&pool, r2, 30000).await;

    let (_, _, outstanding) = supplier_balance(&pool).await;
    assert_eq!(outstanding, 30000);
}

#[tokio::test]
async fn supplier_ledger_utxos_get_version_ids() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let rid = receive_on_credit(&pool, 1, 1, 5.0, 1000).await;
    supplier_pay(&pool, rid, 3000).await;

    let vids: Vec<String> =
        sqlx::query_scalar("SELECT version_id FROM supplier_ledger_utxos ORDER BY id ASC")
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(vids.len(), 2);
    assert_eq!(vids[0].len(), 64, "version_id should be a 64-char hex hash");
    assert_eq!(vids[1].len(), 64);
    assert_ne!(vids[0], vids[1], "version_ids should differ (chained)");
}

#[tokio::test]
async fn supplier_ledger_is_append_only() {
    let pool = test_pool().await;
    seed_base(&pool).await;

    let rid = receive_on_credit(&pool, 1, 1, 5.0, 1000).await;
    supplier_pay(&pool, rid, 2000).await;
    supplier_pay(&pool, rid, 1000).await;

    // Verify all 3 entries exist (1 debt + 2 payments)
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM supplier_ledger_utxos WHERE receipt_id = ?")
            .bind(rid)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(count, 3, "ledger should have 3 entries");

    // Verify the amounts
    let amounts: Vec<i64> = sqlx::query_scalar(
        "SELECT amount FROM supplier_ledger_utxos WHERE receipt_id = ? ORDER BY id ASC",
    )
    .bind(rid)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(amounts, vec![-5000, 2000, 1000]);
}
