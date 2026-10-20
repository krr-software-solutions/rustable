/*!40101 SET NAMES utf8mb4 */;

-- HD-104: auto-close solved tickets after N days. Called by the cron box:
--   CALL sp_auto_close_solved(7);
-- also drops the dead tag table while we're here (tags moved to the search index in HD-99)

DROP PROCEDURE IF EXISTS `sp_auto_close_solved`;

DELIMITER $$

CREATE DEFINER=CURRENT_USER PROCEDURE `sp_auto_close_solved`(IN p_days INT)
    MODIFIES SQL DATA
    COMMENT 'HD-104: solved -> closed after p_days of inactivity'
BEGIN
  DECLARE v_cutoff DATETIME;
  SET v_cutoff = NOW() - INTERVAL p_days DAY;

  UPDATE `tickets`
     SET `status` = 'closed'
   WHERE `status` = 'solved'
     AND `updated_at` < v_cutoff;

  SELECT ROW_COUNT() AS `closed_count`;
END $$

DELIMITER ;

DROP TABLE IF EXISTS `ticket_legacy_tags`;
