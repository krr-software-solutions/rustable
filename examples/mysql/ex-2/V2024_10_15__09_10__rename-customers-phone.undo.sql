-- SHOP-127 rollback: name + width restored, stripped spaces/truncated data are not.
ALTER TABLE `customers`
  CHANGE COLUMN `phone_number` `phone` varchar(32) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT 'free text, E.164 not enforced';
