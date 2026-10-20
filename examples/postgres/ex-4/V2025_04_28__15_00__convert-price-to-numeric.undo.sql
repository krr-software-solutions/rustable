ALTER TABLE products DROP CONSTRAINT products_price_check;
ALTER TABLE products RENAME COLUMN price TO price_cents;
ALTER TABLE products
    ALTER COLUMN price_cents SET DATA TYPE bigint USING round(price_cents * 100)::bigint;
ALTER TABLE products ADD CONSTRAINT products_price_cents_check CHECK (price_cents >= 0);
