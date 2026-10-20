/*!40101 SET NAMES utf8mb4 */;

/*
 * SHOP-162: denormalized per-customer stats for the CRM export.
 * Populated once here; kept fresh by the nightly job afterwards.
 */

CREATE TABLE `customer_order_stats` (
  `customer_id` bigint unsigned NOT NULL,
  `order_count` int unsigned NOT NULL DEFAULT '0',
  `lifetime_cents` bigint NOT NULL DEFAULT '0',
  `last_order_at` datetime DEFAULT NULL,
  `refreshed_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`customer_id`),
  KEY `idx_cos_lifetime` (`lifetime_cents` DESC),
  CONSTRAINT `fk_cos_customer` FOREIGN KEY (`customer_id`) REFERENCES `customers` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC;

INSERT INTO `customer_order_stats` (`customer_id`, `order_count`, `lifetime_cents`, `last_order_at`)
SELECT o.`customer_id`, COUNT(*), SUM(o.`total_cents`), MAX(o.`created_at`)
  FROM `orders` o
 WHERE o.`status` NOT IN ('cancelled')
 GROUP BY o.`customer_id`
    ON DUPLICATE KEY UPDATE
       `order_count`    = VALUES(`order_count`),
       `lifetime_cents` = VALUES(`lifetime_cents`),
       `last_order_at`  = VALUES(`last_order_at`);
