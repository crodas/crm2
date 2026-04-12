-- Teams
CREATE TABLE teams (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    color      TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE team_members (
    id         INTEGER PRIMARY KEY,
    team_id    INTEGER NOT NULL REFERENCES teams(id),
    name       TEXT NOT NULL,
    role       TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Quotes with lifecycle
CREATE TABLE quotes (
    id           INTEGER PRIMARY KEY,
    customer_id  INTEGER NOT NULL REFERENCES customers(id),
    status       TEXT NOT NULL DEFAULT 'draft'
                 CHECK (status IN ('draft','sent','follow_up','accepted','booked')),
    title        TEXT NOT NULL,
    description  TEXT,
    total_amount INTEGER NOT NULL DEFAULT 0,
    is_debt      INTEGER NOT NULL DEFAULT 0,
    valid_until  TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_quotes_customer ON quotes(customer_id);
CREATE INDEX idx_quotes_status ON quotes(status);

CREATE TABLE quote_lines (
    id          INTEGER PRIMARY KEY,
    quote_id    INTEGER NOT NULL REFERENCES quotes(id),
    description TEXT NOT NULL,
    quantity    REAL NOT NULL DEFAULT 1,
    unit_price  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- =============================================
-- PAYMENT UTXO SYSTEM (append-only ledger)
-- =============================================

CREATE TABLE payment_utxos (
    id         INTEGER PRIMARY KEY,
    quote_id   INTEGER NOT NULL REFERENCES quotes(id),
    amount     INTEGER NOT NULL CHECK (amount > 0),
    method     TEXT,
    notes      TEXT,
    paid_at    TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_payment_utxos_quote ON payment_utxos(quote_id);

-- Bookings (scheduled jobs)
CREATE TABLE bookings (
    id          INTEGER PRIMARY KEY,
    team_id     INTEGER NOT NULL REFERENCES teams(id),
    customer_id INTEGER NOT NULL REFERENCES customers(id),
    title       TEXT NOT NULL,
    start_at    TEXT NOT NULL,
    end_at      TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'scheduled'
                CHECK (status IN ('scheduled','in_progress','completed','cancelled')),
    notes       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_bookings_team_date ON bookings(team_id, start_at);
CREATE INDEX idx_bookings_customer ON bookings(customer_id);

-- Booking <-> Quote many-to-many
CREATE TABLE booking_quotes (
    booking_id INTEGER NOT NULL REFERENCES bookings(id),
    quote_id   INTEGER NOT NULL REFERENCES quotes(id),
    PRIMARY KEY (booking_id, quote_id)
);

-- Work orders
CREATE TABLE work_orders (
    id          INTEGER PRIMARY KEY,
    booking_id  INTEGER NOT NULL REFERENCES bookings(id),
    customer_id INTEGER NOT NULL REFERENCES customers(id),
    description TEXT NOT NULL,
    location    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_work_orders_booking ON work_orders(booking_id);
