-- Sale payment tracking: immediate capture vs deferred (credit)
ALTER TABLE sales ADD COLUMN payment_status TEXT NOT NULL DEFAULT 'credit'
    CHECK (payment_status IN ('paid', 'credit'));

CREATE TABLE sale_payments (
    id         INTEGER PRIMARY KEY,
    sale_id    INTEGER NOT NULL REFERENCES sales(id),
    amount     INTEGER NOT NULL CHECK (amount > 0),
    method     TEXT,
    notes      TEXT,
    paid_at    TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
