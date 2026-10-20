/*!40101 SET NAMES utf8mb4 */;

-- HD-22: customers keep sending emoji and latin1 turns them into '????'.
-- CONVERT rebuilds the table (ALGORITHM=COPY under the hood) -- run off-peak.
-- requester_email index is a prefix index so it stays under the 3072 byte limit.

ALTER TABLE `tickets` CONVERT TO CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
ALTER TABLE `agents`  CONVERT TO CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

ALTER TABLE `tickets` COMMENT='support tickets (utf8mb4 since HD-22)', ROW_FORMAT=DYNAMIC;
