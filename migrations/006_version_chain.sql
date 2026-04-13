-- Add version_id hash chain to all append-only tables.
-- version_id = sha256(readonly_fields | prev_version_id)
-- Backfill happens in Rust after migration (version::recompute_all_chains).

ALTER TABLE inventory_receipts ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE inventory_utxos ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE inventory_receipt_prices ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE sales ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE sale_lines ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE sale_line_utxo_inputs ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE payment_utxos ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
