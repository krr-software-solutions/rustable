UPDATE products SET description = '' WHERE description IS NULL;
ALTER TABLE products ALTER COLUMN description SET DEFAULT '';
ALTER TABLE products ALTER COLUMN description SET NOT NULL;

ALTER TABLE inventory ALTER COLUMN updated_at SET DEFAULT now();
ALTER TABLE inventory ALTER COLUMN quantity DROP DEFAULT;

ALTER INDEX inventory_low_stock_idx RENAME TO stock_levels_low_stock_idx;
ALTER TABLE inventory RENAME TO stock_levels;
