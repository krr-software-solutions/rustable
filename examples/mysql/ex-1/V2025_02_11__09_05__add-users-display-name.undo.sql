-- BLOG-31 rollback: drop index first, then the column (backfilled names are lost, they're derivable anyway)
ALTER TABLE `users` DROP INDEX `idx_users_display_name`, ALGORITHM=INPLACE, LOCK=NONE;
ALTER TABLE `users` DROP COLUMN `display_name`;
