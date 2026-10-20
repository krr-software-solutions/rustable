-- HD-120: requesters are now first-class `requesters` rows. Move the data over,
-- then drop the denormalized email column from tickets.
-- Irreversible-ish: ticket->email mapping lives only in requesters now.

CREATE TABLE `requesters` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `email` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL,
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_requesters_email` (`email`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci ROW_FORMAT=DYNAMIC;

INSERT IGNORE INTO `requesters` (`email`)
SELECT DISTINCT LOWER(TRIM(`requester_email`)) FROM `tickets`;

ALTER TABLE `tickets`
  ADD COLUMN `requester_id` bigint unsigned DEFAULT NULL AFTER `assignee_id`,
  ALGORITHM=INSTANT;

UPDATE `tickets` t
  JOIN `requesters` r ON r.`email` = LOWER(TRIM(t.`requester_email`))
   SET t.`requester_id` = r.`id`;

ALTER TABLE `tickets`
  DROP INDEX `idx_tickets_requester`,
  DROP COLUMN `requester_email`,
  ADD KEY `idx_tickets_requester_id` (`requester_id`),
  ADD CONSTRAINT `fk_tickets_requester` FOREIGN KEY (`requester_id`) REFERENCES `requesters` (`id`);
