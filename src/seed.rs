use sqlx::SqlitePool;

pub async fn seed_dev_data(pool: &SqlitePool) -> Result<(), sqlx::Error> {
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
         ('Roof Tile Clay', 'RFT-CL', 'unit', 'product', 0)"
    ).execute(pool).await?;

    // Services (suggested_price in cents)
    sqlx::query(
        "INSERT INTO products (name, sku, unit, product_type, suggested_price) VALUES
         ('Electrical Installation', ?, 'service', 'service', 35000000),
         ('Plumbing Repair', ?, 'service', 'service', 25000000),
         ('Painting (per room)', ?, 'service', 'service', 15000000),
         ('Roof Repair', ?, 'service', 'service', 45000000),
         ('General Maintenance', ?, 'service', 'service', 20000000)"
    )
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .bind(format!("SVC-{}", uuid::Uuid::new_v4()))
    .execute(pool).await?;

    // Warehouses
    sqlx::query(
        "INSERT INTO warehouses (name, address, sort_order) VALUES
         ('Main Warehouse', 'Av. Mariscal Lopez 1234', 1),
         ('Secondary Depot', 'Ruta 2 km 15', 2)"
    ).execute(pool).await?;

    // Customers — retail
    sqlx::query(
        "INSERT INTO customers (customer_type_id, name, phone, email, address) VALUES
         (1, 'Juan Perez', '0981-123456', 'juan@email.com', 'San Lorenzo, Asuncion'),
         (1, 'Maria Garcia', '0982-234567', 'maria@email.com', 'Lambare, Asuncion'),
         (1, 'Carlos Lopez', '0983-345678', 'carlos@email.com', 'Luque'),
         (1, 'Ana Martinez', '0984-456789', 'ana@email.com', 'Fernando de la Mora')"
    ).execute(pool).await?;

    // Customers — resellers
    sqlx::query(
        "INSERT INTO customers (customer_type_id, name, phone, email, address, notes) VALUES
         (2, 'Distribuidora Central', '021-555111', 'ventas@distcentral.com', 'Zona Industrial, Luque', 'Large volume buyer'),
         (2, 'Ferreteria El Constructor', '021-555222', 'compras@constructor.com', 'Av. Eusebio Ayala 4500', 'Weekly orders'),
         (2, 'Materiales del Este', '061-555333', 'info@mateste.com', 'Ciudad del Este', 'Cross-border sales')"
    ).execute(pool).await?;

    // Inventory receipts with prices (all in cents)
    // Receipt 1: Cement and Steel
    sqlx::query("INSERT INTO inventory_receipts (id, reference, supplier_name) VALUES (1, 'INV-2026-001', 'Cementos Paraguayos SA')")
        .execute(pool).await?;

    // Cement: 200 bags at cost 4500000 cents (45000.00), warehouse 1
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (1, 1, 200, 4500000, 1)")
        .execute(pool).await?;
    // Steel: 500 units at cost 1200000 cents (12000.00), warehouse 1
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (2, 1, 500, 1200000, 1)")
        .execute(pool).await?;

    // Prices for receipt 1 — retail (group 1) and reseller (group 2)
    sqlx::query("INSERT INTO inventory_receipt_prices (receipt_id, product_id, customer_group_id, price_per_unit) VALUES
        (1, 1, 1, 6300000), (1, 1, 2, 5400000),
        (1, 2, 1, 1680000), (1, 2, 2, 1440000)")
        .execute(pool).await?;

    // Receipt 2: Sand, Bricks, PVC
    sqlx::query("INSERT INTO inventory_receipts (id, reference, supplier_name) VALUES (2, 'INV-2026-002', 'Aridos del Paraguay')")
        .execute(pool).await?;

    // Sand: 50 tons at cost 15000000 (150000.00), warehouse 1
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (3, 1, 50, 15000000, 2)")
        .execute(pool).await?;
    // Bricks: 10000 at cost 80000 (800.00), warehouse 2
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (4, 2, 10000, 80000, 2)")
        .execute(pool).await?;
    // PVC: 1000 meters at cost 350000 (3500.00), warehouse 1
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (5, 1, 1000, 350000, 2)")
        .execute(pool).await?;

    // Prices for receipt 2
    sqlx::query("INSERT INTO inventory_receipt_prices (receipt_id, product_id, customer_group_id, price_per_unit) VALUES
        (2, 3, 1, 21000000), (2, 3, 2, 18000000),
        (2, 4, 1, 112000),   (2, 4, 2, 96000),
        (2, 5, 1, 490000),   (2, 5, 2, 420000)")
        .execute(pool).await?;

    // Receipt 3: Electrical, Paint, Roof tiles
    sqlx::query("INSERT INTO inventory_receipts (id, reference, supplier_name) VALUES (3, 'INV-2026-003', 'Importadora Electrica')")
        .execute(pool).await?;

    // Cable: 2000m at 150000 (1500.00), warehouse 1
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (6, 1, 2000, 150000, 3)")
        .execute(pool).await?;
    // Paint: 100 buckets at 18000000 (180000.00), warehouse 2
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (7, 2, 100, 18000000, 3)")
        .execute(pool).await?;
    // Roof tiles: 5000 at 250000 (2500.00), warehouse 2
    sqlx::query("INSERT INTO inventory_utxos (product_id, warehouse_id, quantity, cost_per_unit, receipt_id) VALUES (8, 2, 5000, 250000, 3)")
        .execute(pool).await?;

    // Prices for receipt 3
    sqlx::query("INSERT INTO inventory_receipt_prices (receipt_id, product_id, customer_group_id, price_per_unit) VALUES
        (3, 6, 1, 210000),    (3, 6, 2, 180000),
        (3, 7, 1, 25200000),  (3, 7, 2, 21600000),
        (3, 8, 1, 350000),    (3, 8, 2, 300000)")
        .execute(pool).await?;

    // Teams
    sqlx::query("INSERT INTO teams (name, color) VALUES ('Electrical Team', '#e74c3c'), ('Plumbing Team', '#3498db'), ('General Works', '#2ecc71')")
        .execute(pool).await?;

    tracing::info!("Dev data seeded successfully");
    Ok(())
}
