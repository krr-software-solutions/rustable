-- HD-104 rollback.
DROP PROCEDURE IF EXISTS `sp_auto_close_solved`;
-- NOTE: ticket_legacy_tags is recreated empty; the tag data is gone.
CREATE TABLE IF NOT EXISTS `ticket_legacy_tags` (
  `ticket_id` bigint unsigned NOT NULL,
  `tag` varchar(50) NOT NULL,
  PRIMARY KEY (`ticket_id`,`tag`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1 COMMENT='comma-split tags from the PHP app';
