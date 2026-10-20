/*!40101 SET NAMES utf8mb4 */;

-- SHOP-104 orders
-- legacy_ref: old POS order number, only kept until the reconciliation job is retired (SHOP-150)

CREATE TABLE `orders` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `customer_id` bigint unsigned NOT NULL,
  `legacy_ref` varchar(40) CHARACTER SET ascii COLLATE ascii_general_ci DEFAULT NULL COMMENT 'DEPRECATED - POS order no.',
  `status` enum('pending','paid','shipped','delivered','cancelled','refunded') COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT 'pending',
  `currency` char(3) CHARACTER SET ascii COLLATE ascii_bin NOT NULL DEFAULT 'USD',
  `total_cents` bigint NOT NULL DEFAULT '0' COMMENT 'minor units; can be negative for refunds',
  `notes` text COLLATE utf8mb4_unicode_ci,
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  KEY `idx_orders_customer` (`customer_id`),
  KEY `idx_orders_status` (`status`),
  KEY `idx_orders_legacy_ref` (`legacy_ref`),
  CONSTRAINT `fk_orders_customer` FOREIGN KEY (`customer_id`) REFERENCES `customers` (`id`) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC COMMENT='customer orders';

INSERT INTO `orders` (`customer_id`, `legacy_ref`, `status`, `total_cents`)
SELECT c.`id`, CONCAT('POS-', LPAD(c.`id`, 6, '0')), 'delivered', 4999
  FROM `customers` c;
