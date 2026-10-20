/*!40101 SET NAMES utf8mb4 */;

-- ---------------------------------------------------------------------------
-- BLOG-12: initial users table
-- owner: platform-team   reviewed-by: @dmitri
-- ---------------------------------------------------------------------------

CREATE TABLE `users` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT COMMENT 'surrogate PK',
  `email` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT 'login email, case-insensitive via collation',
  `password_hash` varbinary(60) NOT NULL COMMENT 'bcrypt',
  `is_admin` tinyint(1) NOT NULL DEFAULT '0',
  `created_at` datetime(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  `updated_at` datetime(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_users_email` (`email`),
  KEY `idx_users_created_at` (`created_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC COMMENT='blog authors + readers';
