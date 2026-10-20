-- HD-57: the mobile client can't render >64 char subjects. Move overflow into the body,
-- then narrow the column. !!! BREAKING for any writer still sending long subjects !!!

UPDATE `tickets`
   SET `body`    = CONCAT('[original subject] ', `subject`, '\n\n', COALESCE(`body`, '')),
       `subject` = CONCAT(LEFT(`subject`, 61), '...')
 WHERE CHAR_LENGTH(`subject`) > 64;

ALTER TABLE `tickets`
  MODIFY COLUMN `subject` varchar(64) COLLATE utf8mb4_unicode_ci NOT NULL COMMENT 'max 64 since HD-57',
  ALGORITHM=COPY, LOCK=SHARED;
