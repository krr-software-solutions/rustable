-- Naming cleanup ahead of the multi-bin feature.
ALTER TABLE stock_levels RENAME TO inventory;
ALTER INDEX stock_levels_low_stock_idx RENAME TO inventory_low_stock_idx;

-- New rows start at zero instead of requiring an explicit quantity.
ALTER TABLE inventory ALTER COLUMN quantity SET DEFAULT 0;

-- updated_at is now set by the app (with its own clock), not the DB.
ALTER TABLE inventory ALTER COLUMN updated_at DROP DEFAULT;

-- Descriptions are optional again; '' was a poor sentinel.
ALTER TABLE products ALTER COLUMN description DROP NOT NULL;
ALTER TABLE products ALTER COLUMN description DROP DEFAULT;
UPDATE products SET description = NULL WHERE description = '';
