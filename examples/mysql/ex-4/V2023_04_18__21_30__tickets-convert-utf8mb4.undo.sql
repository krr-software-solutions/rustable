-- HD-22 rollback. LOSSY: any 4-byte chars (emoji) and non-latin1 text become '?'
-- (or the ALTER fails outright under strict mode -- clean the data first).
-- `body` was text in latin1 -> became mediumtext on convert; we put it back to text.
ALTER TABLE `agents`  CONVERT TO CHARACTER SET latin1 COLLATE latin1_swedish_ci;
ALTER TABLE `tickets` CONVERT TO CHARACTER SET latin1 COLLATE latin1_swedish_ci;
ALTER TABLE `tickets` MODIFY COLUMN `body` text, COMMENT='', ROW_FORMAT=DEFAULT;
