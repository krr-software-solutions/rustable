-- HD-81: "agents" -> "staff_members" (HR wanted the rename, don't ask).
-- FK on tickets.assignee_id follows the table automatically.
-- !!! old app code queries `agents` directly -> breaks. Ship app v5 in the same window. !!!

RENAME TABLE `agents` TO `staff_members`;

ALTER TABLE `staff_members`
  ADD COLUMN `role` enum('agent','lead','admin') COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT 'agent' AFTER `email`,
  ALGORITHM=INSTANT;

ALTER TABLE `staff_members` COMMENT='helpdesk staff (was `agents` before HD-81)';
