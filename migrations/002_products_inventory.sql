-- Customer groups for pricing tiers
CREATE TABLE customer_groups (
    id                 INTEGER PRIMARY KEY,
    name               TEXT NOT NULL UNIQUE,
    customer_type_id   INTEGER NOT NULL REFERENCES customer_types(id),
    default_markup_pct REAL NOT NULL DEFAULT 0.0,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO customer_groups (name, customer_type_id, default_markup_pct)
VALUES
    ('retail', (SELECT id FROM customer_types WHERE name='retail'), 40.0),
    ('reseller', (SELECT id FROM customer_types WHERE name='reseller'), 20.0);

-- Products catalog
CREATE TABLE products (
    id          INTEGER PRIMARY KEY,
    sku         TEXT UNIQUE,
    name        TEXT NOT NULL,
    description TEXT,
    unit        TEXT NOT NULL DEFAULT 'unit',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Warehouses
CREATE TABLE warehouses (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    address     TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- =============================================
-- UTXO INVENTORY SYSTEM
-- =============================================

-- Purchase / inventory receipt events
CREATE TABLE inventory_receipts (
    id            INTEGER PRIMARY KEY,
    reference     TEXT,
    supplier_name TEXT,
    notes         TEXT,
    received_at   TEXT NOT NULL DEFAULT (datetime('now')),
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Sales (spending transactions)
CREATE TABLE sales (
    id                INTEGER PRIMARY KEY,
    customer_id       INTEGER NOT NULL REFERENCES customers(id),
    customer_group_id INTEGER NOT NULL REFERENCES customer_groups(id),
    notes             TEXT,
    total_amount      INTEGER NOT NULL DEFAULT 0,
    sold_at           TEXT NOT NULL DEFAULT (datetime('now')),
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_sales_customer ON sales(customer_id);

-- UTXO outputs: append-only, never updated except spent flag
CREATE TABLE inventory_utxos (
    id               INTEGER PRIMARY KEY,
    product_id       INTEGER NOT NULL REFERENCES products(id),
    warehouse_id     INTEGER NOT NULL REFERENCES warehouses(id),
    quantity         REAL NOT NULL CHECK (quantity > 0),
    cost_per_unit    INTEGER NOT NULL CHECK (cost_per_unit >= 0),

    -- Origin
    receipt_id       INTEGER REFERENCES inventory_receipts(id),
    source_sale_id   INTEGER REFERENCES sales(id),

    -- Spending status (only mutation: 0 -> 1)
    spent            INTEGER NOT NULL DEFAULT 0,
    spent_by_sale_id INTEGER REFERENCES sales(id),

    created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Critical: fast lookup of current unspent stock
CREATE INDEX idx_utxo_unspent ON inventory_utxos(product_id, warehouse_id) WHERE spent = 0;
CREATE INDEX idx_utxo_receipt ON inventory_utxos(receipt_id);

-- Prices per receipt line per customer group
CREATE TABLE inventory_receipt_prices (
    id                INTEGER PRIMARY KEY,
    receipt_id        INTEGER NOT NULL REFERENCES inventory_receipts(id),
    product_id        INTEGER NOT NULL REFERENCES products(id),
    customer_group_id INTEGER NOT NULL REFERENCES customer_groups(id),
    price_per_unit    INTEGER NOT NULL CHECK (price_per_unit >= 0),
    UNIQUE(receipt_id, product_id, customer_group_id)
);

-- Sale line items
CREATE TABLE sale_lines (
    id             INTEGER PRIMARY KEY,
    sale_id        INTEGER NOT NULL REFERENCES sales(id),
    product_id     INTEGER NOT NULL REFERENCES products(id),
    quantity       REAL NOT NULL CHECK (quantity > 0),
    price_per_unit INTEGER NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_sale_lines_sale ON sale_lines(sale_id);

-- Audit: which UTXOs were consumed by which sale line
CREATE TABLE sale_line_utxo_inputs (
    id            INTEGER PRIMARY KEY,
    sale_line_id  INTEGER NOT NULL REFERENCES sale_lines(id),
    utxo_id       INTEGER NOT NULL REFERENCES inventory_utxos(id),
    quantity_used REAL NOT NULL CHECK (quantity_used > 0)
);
