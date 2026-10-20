-- SHOP-140: guest checkout now always collects an email. Backfill the
-- legacy guests with a synthetic, undeliverable address, then enforce NOT NULL.
-- !!! old app (<= v2.9) can still INSERT customers without email -> will now fail !!!

UPDATE `customers`
   SET `email` = CONCAT('guest+', `external_ref`, '@invalid.example')
 WHERE `email` IS NULL;

ALTER TABLE `customers`
  MODIFY COLUMN `email` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT 'required since SHOP-140',
  ALGORITHM=INPLACE, LOCK=SHARED;
