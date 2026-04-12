PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- System configuration (key/value)
CREATE TABLE config (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO config (key, value) VALUES
    ('currency', 'PYG'),
    ('currency_symbol', '₲'),
    ('currency_decimals', '0'),
    ('company_name', ''),
    ('company_address', ''),
    ('company_phone', ''),
    ('company_tax_id', ''),
    ('default_payment_methods', '["cash","card","transfer","check"]'),
    ('quote_validity_days', '30'),
    ('quote_followup_days', '7'),
    ('inventory_costing_method', 'fifo'),
    ('units', '["unit","kg","meter","liter","box"]');

-- Configurable customer types
CREATE TABLE customer_types (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO customer_types (name) VALUES ('retail'), ('reseller');

-- Customers
CREATE TABLE customers (
    id               INTEGER PRIMARY KEY,
    customer_type_id INTEGER NOT NULL REFERENCES customer_types(id),
    name             TEXT NOT NULL,
    email            TEXT,
    phone            TEXT,
    address          TEXT,
    notes            TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_customers_type ON customers(customer_type_id);
