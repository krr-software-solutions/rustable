-- BLOG-31: display names. Nullable on purpose -- old app pods don't know about it yet.
-- INSTANT so we don't rebuild the table during business hours.

ALTER TABLE `users`
  ADD COLUMN `display_name` varchar(64) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT 'shown on posts; falls back to email local-part' AFTER `email`,
  ALGORITHM=INSTANT;

/* backfill: local part of the email, e.g. "jane.doe@x.com" -> "jane.doe" */
UPDATE `users`
   SET `display_name` = LEFT(SUBSTRING_INDEX(`email`, '@', 1), 64)
 WHERE `display_name` IS NULL;

ALTER TABLE `users` ADD INDEX `idx_users_display_name` (`display_name`(20)), ALGORITHM=INPLACE, LOCK=NONE;
