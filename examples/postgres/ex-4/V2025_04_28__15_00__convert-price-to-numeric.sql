/*
 * Finance wants prices as decimal currency amounts, not integer cents.
 * Rewrites the whole table (ACCESS EXCLUSIVE lock) -- run off-peak.
 */
ALTER TABLE products DROP CONSTRAINT products_price_cents_check;

ALTER TABLE products
    ALTER COLUMN price_cents SET DATA TYPE numeric(12, 2) USING (price_cents / 100.0);

ALTER TABLE products RENAME COLUMN price_cents TO price;

ALTER TABLE products ADD CONSTRAINT products_price_check CHECK (price >= 0);
