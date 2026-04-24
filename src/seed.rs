use crate::state::AppState;

pub async fn seed_dev_data(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
    let pool = &state.pool;

    // Check if already seeded
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    tracing::info!("Seeding dev data...");

    // Products (prices in cents)
    sqlx::query(
        "INSERT INTO products (name, sku, unit, product_type, suggested_price) VALUES
         ('Cement 50kg', 'CEM-50', 'bag', 'product', 0),
         ('Steel Rod 12mm', 'STL-12', 'unit', 'product', 0),
         ('Sand (fine)', 'SND-F', 'ton', 'product', 0),
         ('Brick Standard', 'BRK-01', 'unit', 'product', 0),
         ('PVC Pipe 4\"', 'PVC-4', 'meter', 'product', 0),
         ('Electrical Cable 2.5mm', 'ELC-25', 'meter', 'product', 0),
         ('Paint White 20L', 'PNT-W20', 'bucket', 'product', 0),
         ('Roof Tile Clay', 'RFT-CL', 'unit', 'product', 0)",
    )
    .execute(pool)
    .await?;

    // Services (suggested_price in cents)
    sqlx::query(
        "INSERT INTO products (name, sku, unit, product_type, suggested_price) VALUES
         ('Electrical Installation', ?, 'service', 'service', 35000000),
         ('Plumbing Repair', ?, 'service', 'service', 25000000),
         ('Painting (per room)', ?, 'service', 'service', 15000000),
         ('Roof Repair', ?, 'service', 'service', 45000000),
         ('General Maintenance', ?, 'service', 'service', 20000000)",
    )
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .execute(pool)
    .await?;

    // Register product assets in ledger
    for id in 1..=8 {
        state
            .ledger
            .register_asset(ledger::Asset::new(format!("product:{id}"), 3))
            .await?;
    }

    // Warehouses
    sqlx::query(
        "INSERT INTO warehouses (name, address, sort_order) VALUES
         ('Main Warehouse', 'Av. Mariscal Lopez 1234', 1),
         ('Secondary Depot', 'Ruta 2 km 15', 2)",
    )
    .execute(pool)
    .await?;

    // Customers — retail
    sqlx::query(
        "INSERT INTO customers (customer_type_id, name, phone, email, address) VALUES
         (1, 'Juan Perez', '0981-123456', 'juan@email.com', 'San Lorenzo, Asuncion'),
         (1, 'Maria Garcia', '0982-234567', 'maria@email.com', 'Lambare, Asuncion'),
         (1, 'Carlos Lopez', '0983-345678', 'carlos@email.com', 'Luque'),
         (1, 'Ana Martinez', '0984-456789', 'ana@email.com', 'Fernando de la Mora')",
    )
    .execute(pool)
    .await?;

    // Customers — resellers
    sqlx::query(
        "INSERT INTO customers (customer_type_id, name, phone, email, address, notes) VALUES
         (2, 'Distribuidora Central', '021-555111', 'ventas@distcentral.com', 'Zona Industrial, Luque', 'Large volume buyer'),
         (2, 'Ferreteria El Constructor', '021-555222', 'compras@constructor.com', 'Av. Eusebio Ayala 4500', 'Weekly orders'),
         (2, 'Materiales del Este', '061-555333', 'info@mateste.com', 'Ciudad del Este', 'Cross-border sales')"
    ).execute(pool).await?;

    // Inventory receipts with lines and ledger entries
    // Receipt 1: Cement and Steel
    seed_receipt(
        state,
        1,
        "INV-2026-001",
        "Cementos Paraguayos SA",
        &[
            // (product_id, warehouse_id, quantity, cost_per_unit_cents)
            (1, 1, 200.0, 4500000),
            (2, 1, 500.0, 1200000),
        ],
        &[
            // (product_id, customer_group_id, price_per_unit_cents)
            (1, 1, 6300000),
            (1, 2, 5400000),
            (2, 1, 1680000),
            (2, 2, 1440000),
        ],
    )
    .await?;

    // Receipt 2: Sand, Bricks, PVC
    seed_receipt(
        state,
        2,
        "INV-2026-002",
        "Aridos del Paraguay",
        &[
            (3, 1, 50.0, 15000000),
            (4, 2, 10000.0, 80000),
            (5, 1, 1000.0, 350000),
        ],
        &[
            (3, 1, 21000000),
            (3, 2, 18000000),
            (4, 1, 112000),
            (4, 2, 96000),
            (5, 1, 490000),
            (5, 2, 420000),
        ],
    )
    .await?;

    // Receipt 3: Electrical, Paint, Roof tiles
    seed_receipt(
        state,
        3,
        "INV-2026-003",
        "Importadora Electrica",
        &[
            (6, 1, 2000.0, 150000),
            (7, 2, 100.0, 18000000),
            (8, 2, 5000.0, 250000),
        ],
        &[
            (6, 1, 210000),
            (6, 2, 180000),
            (7, 1, 25200000),
            (7, 2, 21600000),
            (8, 1, 350000),
            (8, 2, 300000),
        ],
    )
    .await?;

    // Teams
    sqlx::query("INSERT INTO teams (name, color) VALUES ('Electrical Team', '#e74c3c'), ('Plumbing Team', '#3498db'), ('General Works', '#2ecc71')")
        .execute(pool).await?;

    tracing::info!("Dev data seeded successfully");
    Ok(())
}

async fn seed_receipt(
    state: &AppState,
    receipt_id: i64,
    reference: &str,
    supplier: &str,
    lines: &[(i64, i64, f64, i64)], // (product_id, warehouse_id, qty, cost_cents)
    prices: &[(i64, i64, i64)],     // (product_id, group_id, price_cents)
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = &state.pool;

    // Calculate total cost
    let total_cost: i64 = lines
        .iter()
        .map(|&(_, _, qty, cost)| (qty * cost as f64).round() as i64)
        .sum();

    sqlx::query(
        "INSERT INTO inventory_receipts (id, reference, supplier_name, total_cost) VALUES (?, ?, ?, ?)",
    )
    .bind(receipt_id)
    .bind(reference)
    .bind(supplier)
    .bind(total_cost)
    .execute(pool)
    .await?;

    // Build ledger transaction for inventory tokens
    let mut builder = state.ledger.transaction(format!("receipt-{receipt_id}"));

    for &(product_id, warehouse_id, qty, cost) in lines {
        // Store line item metadata
        sqlx::query(
            "INSERT INTO inventory_receipt_lines (receipt_id, product_id, warehouse_id, quantity, cost_per_unit)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(receipt_id)
        .bind(product_id)
        .bind(warehouse_id)
        .bind(qty)
        .bind(cost)
        .execute(pool)
        .await?;

        // Credit inventory to the store warehouse
        let account = format!("store/{warehouse_id}");
        let asset = state
            .ledger
            .asset(&format!("product:{product_id}"))
            .ok_or_else(|| -> Box<dyn std::error::Error> {
                format!("asset product:{product_id} not registered").into()
            })?;
        let amount = asset.parse_amount(&format!("{qty:.3}"))?;
        builder = builder.issue(&account, &amount)?;
    }

    // Commit ledger transaction
    let ledger_tx = builder.build().await?;
    state.ledger.commit(ledger_tx).await?;

    // Store prices
    for &(product_id, group_id, price) in prices {
        sqlx::query(
            "INSERT INTO inventory_receipt_prices (receipt_id, product_id, customer_group_id, price_per_unit)
             VALUES (?, ?, ?, ?)",
        )
        .bind(receipt_id)
        .bind(product_id)
        .bind(group_id)
        .bind(price)
        .execute(pool)
        .await?;
    }

    Ok(())
}
