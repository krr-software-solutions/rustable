/*!40101 SET NAMES utf8mb4 */;
/*!40101 SET @OLD_CHARACTER_SET_CLIENT=@@CHARACTER_SET_CLIENT */;

-- =====================================================================
-- SHOP-101  customers
-- NOTE(ops): email is nullable because the guest-checkout import
--            (legacy POS) has rows without one. Revisit in SHOP-140.
-- =====================================================================

CREATE TABLE `customers` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `external_ref` char(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL COMMENT 'uuid from legacy POS',
  `email` varchar(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL,
  `full_name` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT '',
  `phone` varchar(32) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT 'free text, E.164 not enforced',
  `marketing_opt_in` tinyint(1) NOT NULL DEFAULT '0',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_customers_external_ref` (`external_ref`),
  KEY `idx_customers_email` (`email`(20))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC COMMENT='shop customers (incl. guests)';

INSERT INTO `customers` (`external_ref`, `email`, `full_name`, `phone`) VALUES
  ('00000000-0000-0000-0000-000000000001', 'alice@example.com', 'Alice Example', '+1 555 0100'),
  ('00000000-0000-0000-0000-000000000002', NULL, 'Walk-in Guest', NULL);

/*!40101 SET CHARACTER_SET_CLIENT=@OLD_CHARACTER_SET_CLIENT */;
