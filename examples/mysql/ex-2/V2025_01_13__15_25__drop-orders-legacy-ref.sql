-- SHOP-150: POS reconciliation job retired 2025-01-10. Bye legacy_ref.
-- Irreversible: the POS numbers are gone for good (snapshot taken: s3://backups/shop/orders_legacy_ref_2025-01-13.csv)

ALTER TABLE `orders` DROP INDEX `idx_orders_legacy_ref`, ALGORITHM=INPLACE, LOCK=NONE;
ALTER TABLE `orders` DROP COLUMN `legacy_ref`, ALGORITHM=INSTANT;
