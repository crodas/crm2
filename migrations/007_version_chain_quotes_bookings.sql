-- Add version_id hash chain to quotes, quote_lines, bookings, and work_orders.
-- Backfill happens in Rust after migration (version::recompute_all_chains).

ALTER TABLE quotes ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE quote_lines ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE bookings ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
ALTER TABLE work_orders ADD COLUMN version_id TEXT NOT NULL DEFAULT '';
