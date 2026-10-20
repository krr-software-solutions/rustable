-- HD-81 rollback (role assignments are lost)
ALTER TABLE `staff_members` DROP COLUMN `role`, ALGORITHM=INSTANT;
ALTER TABLE `staff_members` COMMENT='';
RENAME TABLE `staff_members` TO `agents`;
