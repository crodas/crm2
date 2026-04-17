-- Receipt line items (replaces inventory_utxos for metadata)
CREATE TABLE IF NOT EXISTS inventory_receipt_lines (
    id            INTEGER PRIMARY KEY,
    receipt_id    INTEGER NOT NULL REFERENCES inventory_receipts(id),
    product_id    INTEGER NOT NULL REFERENCES products(id),
    warehouse_id  INTEGER NOT NULL REFERENCES warehouses(id),
    quantity      REAL NOT NULL CHECK (quantity > 0),
    cost_per_unit INTEGER NOT NULL CHECK (cost_per_unit >= 0),
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_receipt_lines_receipt ON inventory_receipt_lines(receipt_id);

-- Backfill from inventory_utxos before dropping
INSERT OR IGNORE INTO inventory_receipt_lines (receipt_id, product_id, warehouse_id, quantity, cost_per_unit, created_at)
SELECT receipt_id, product_id, warehouse_id, quantity, cost_per_unit, created_at
FROM inventory_utxos
WHERE receipt_id IS NOT NULL;

-- Drop tables replaced by the ledger engine
DROP TABLE IF EXISTS sale_line_utxo_inputs;
DROP TABLE IF EXISTS inventory_utxos;

-- Drop vestigial version_id columns (no longer computed)
ALTER TABLE inventory_receipts DROP COLUMN version_id;
ALTER TABLE inventory_receipt_prices DROP COLUMN version_id;
ALTER TABLE supplier_ledger_utxos DROP COLUMN version_id;
ALTER TABLE sales DROP COLUMN version_id;
ALTER TABLE sale_lines DROP COLUMN version_id;
ALTER TABLE payment_utxos DROP COLUMN version_id;
