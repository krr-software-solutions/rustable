/*!40101 SET @OLD_CHARACTER_SET_CLIENT=@@CHARACTER_SET_CLIENT */;
/*!40101 SET NAMES utf8mb4 */;
/*!40014 SET @OLD_FOREIGN_KEY_CHECKS=@@FOREIGN_KEY_CHECKS, FOREIGN_KEY_CHECKS=0 */;

-- ###########################################################################
-- FLAGS-1  bootstrap schema for the feature-flag service
-- Generated from mysqldump of the prototype, hand-edited. Tables are created
-- child-first on purpose (FK checks are off for the whole file).
-- ###########################################################################

CREATE TABLE `flag_overrides` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `flag_id` int unsigned NOT NULL,
  `environment_id` smallint unsigned NOT NULL,
  `subject_type` enum('user','org','ip_range') COLLATE utf8mb4_unicode_ci NOT NULL,
  `subject_key` varchar(191) COLLATE utf8mb4_unicode_ci NOT NULL,
  `enabled` tinyint(1) NOT NULL,
  `expires_at` datetime DEFAULT NULL COMMENT 'NULL = forever',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_override` (`flag_id`,`environment_id`,`subject_type`,`subject_key`),
  KEY `idx_override_env` (`environment_id`),
  KEY `idx_override_expiry` (`expires_at`),
  CONSTRAINT `fk_override_flag` FOREIGN KEY (`flag_id`) REFERENCES `flags` (`id`) ON DELETE CASCADE,
  CONSTRAINT `fk_override_env` FOREIGN KEY (`environment_id`) REFERENCES `environments` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC COMMENT='per-subject overrides';

CREATE TABLE `flags` (
  `id` int unsigned NOT NULL AUTO_INCREMENT,
  `key` varchar(128) CHARACTER SET ascii COLLATE ascii_bin NOT NULL COMMENT 'e.g. checkout.new-flow',
  `description` varchar(1024) COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT '',
  `default_enabled` tinyint(1) NOT NULL DEFAULT '0',
  `rollout_pct` decimal(5,2) unsigned NOT NULL DEFAULT '0.00',
  `created_at` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_flags_key` (`key`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC;

CREATE TABLE `environments` (
  `id` smallint unsigned NOT NULL AUTO_INCREMENT,
  `name` varchar(32) COLLATE utf8mb4_unicode_ci NOT NULL,
  `is_production` tinyint(1) NOT NULL DEFAULT '0',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_env_name` (`name`)
) ENGINE=InnoDB AUTO_INCREMENT=10 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

LOCK TABLES `environments` WRITE, `flags` WRITE;
/*!40000 ALTER TABLE `environments` DISABLE KEYS */;
INSERT INTO `environments` (`id`, `name`, `is_production`) VALUES (1,'dev',0),(2,'staging',0),(3,'prod',1);
/*!40000 ALTER TABLE `environments` ENABLE KEYS */;
INSERT INTO `flags` (`key`, `description`, `default_enabled`, `rollout_pct`) VALUES
  ('checkout.new-flow', 'React checkout rewrite', 0, 5.00),
  ('search.typo-tolerance', 'fuzzy search', 1, 100.00);
UNLOCK TABLES;

/*!40014 SET FOREIGN_KEY_CHECKS=@OLD_FOREIGN_KEY_CHECKS */;
/*!40101 SET CHARACTER_SET_CLIENT=@OLD_CHARACTER_SET_CLIENT */;
