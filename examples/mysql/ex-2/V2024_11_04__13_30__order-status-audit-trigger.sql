/*!40101 SET NAMES utf8mb4 */;

-- SHOP-133: compliance wants every status transition recorded.
-- Run via the mysql CLI (DELIMITER is a client directive!)

CREATE TABLE `order_status_audit` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `order_id` bigint unsigned NOT NULL,
  `old_status` varchar(16) COLLATE utf8mb4_unicode_ci DEFAULT NULL,
  `new_status` varchar(16) COLLATE utf8mb4_unicode_ci NOT NULL,
  `changed_at` timestamp(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  `changed_by` varchar(128) COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT '' COMMENT 'CURRENT_USER() at time of change',
  PRIMARY KEY (`id`),
  KEY `idx_osa_order` (`order_id`, `changed_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC COMMENT='append-only; no FK so audit survives order deletes';

DROP TRIGGER IF EXISTS `trg_orders_status_audit`;

DELIMITER $$
CREATE TRIGGER `trg_orders_status_audit`
AFTER UPDATE ON `orders`
FOR EACH ROW
BEGIN
  IF NOT (OLD.`status` <=> NEW.`status`) THEN
    INSERT INTO `order_status_audit` (`order_id`, `old_status`, `new_status`, `changed_by`)
    VALUES (NEW.`id`, OLD.`status`, NEW.`status`, CURRENT_USER());
  END IF;
END$$
DELIMITER ;
