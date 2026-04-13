-- Merge work_orders into bookings: add description and location columns,
-- migrate existing work_order data, then drop work_orders table.

ALTER TABLE bookings ADD COLUMN description TEXT;
ALTER TABLE bookings ADD COLUMN location TEXT;

-- Migrate: concatenate multiple work order descriptions per booking (newline-separated),
-- take the first non-null location.
UPDATE bookings SET
    description = (
        SELECT GROUP_CONCAT(wo.description, char(10))
        FROM work_orders wo WHERE wo.booking_id = bookings.id
    ),
    location = (
        SELECT wo.location FROM work_orders wo
        WHERE wo.booking_id = bookings.id AND wo.location IS NOT NULL
        LIMIT 1
    )
WHERE EXISTS (SELECT 1 FROM work_orders wo WHERE wo.booking_id = bookings.id);

DROP INDEX IF EXISTS idx_work_orders_booking;
DROP TABLE work_orders;
