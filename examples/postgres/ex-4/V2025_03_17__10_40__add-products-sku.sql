-- Step 1: nullable column, so the running app is unaffected.
ALTER TABLE products ADD COLUMN sku text;

-- Step 2: backfill existing rows with a deterministic placeholder SKU.
UPDATE products SET sku = 'LEGACY-' || lpad(id::text, 6, '0') WHERE sku IS NULL;

-- Step 3: enforce it. Breaking: old app code inserting products without a sku now fails.
ALTER TABLE products ALTER COLUMN sku SET NOT NULL;

CREATE UNIQUE INDEX CONCURRENTLY products_sku_key ON products (sku);

-- While we're here: the search box does case-insensitive name lookups.
CREATE INDEX CONCURRENTLY products_name_lower_idx ON products (lower(name));
