-- Supplier ledger: track debt and payments for inventory receipts
-- Negative amount = debt (money owed to supplier)
-- Positive amount = payment (money paid to supplier)

ALTER TABLE inventory_receipts ADD COLUMN total_cost INTEGER NOT NULL DEFAULT 0;

-- Backfill total_cost for existing receipts
UPDATE inventory_receipts SET total_cost = COALESCE(
  (SELECT CAST(SUM(ROUND(quantity * cost_per_unit)) AS INTEGER)
   FROM inventory_utxos WHERE receipt_id = inventory_receipts.id), 0);

CREATE TABLE supplier_ledger_utxos (
    id         INTEGER PRIMARY KEY,
    receipt_id INTEGER NOT NULL REFERENCES inventory_receipts(id),
    amount     INTEGER NOT NULL,
    method     TEXT,
    notes      TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    version_id TEXT NOT NULL DEFAULT ''
);

CREATE INDEX idx_supplier_ledger_receipt ON supplier_ledger_utxos(receipt_id);
