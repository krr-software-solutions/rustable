-- FLAGS-1 rollback: nukes the whole service schema.
SET FOREIGN_KEY_CHECKS=0;
DROP TABLE IF EXISTS `flag_overrides`, `flags`, `environments`;
SET FOREIGN_KEY_CHECKS=1;
