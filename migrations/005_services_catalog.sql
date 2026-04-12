-- Unify products and services in the same table
ALTER TABLE products ADD COLUMN product_type TEXT NOT NULL DEFAULT 'product';
ALTER TABLE products ADD COLUMN suggested_price INTEGER NOT NULL DEFAULT 0;

-- Add optional service_id to quote_lines to link to products/services
ALTER TABLE quote_lines ADD COLUMN service_id INTEGER REFERENCES products(id);
-- Add a type discriminator: 'service' or 'item'
ALTER TABLE quote_lines ADD COLUMN line_type TEXT NOT NULL DEFAULT 'item';
