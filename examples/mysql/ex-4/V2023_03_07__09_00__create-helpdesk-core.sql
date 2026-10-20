-- HD-1: helpdesk core tables.
-- NB: copied the charset from the old PHP app's DB, which was latin1. Yes really.

CREATE TABLE `agents` (
  `id` int unsigned NOT NULL AUTO_INCREMENT,
  `username` varchar(64) NOT NULL,
  `email` varchar(255) NOT NULL,
  `active` tinyint(1) NOT NULL DEFAULT '1',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_agents_username` (`username`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1 COLLATE=latin1_swedish_ci;

CREATE TABLE `tickets` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `subject` varchar(255) NOT NULL,
  `body` text,
  `status` enum('open','pending','solved','closed') NOT NULL DEFAULT 'open',
  `priority` tinyint unsigned NOT NULL DEFAULT '3' COMMENT '1=urgent .. 4=low',
  `assignee_id` int unsigned DEFAULT NULL,
  `requester_email` varchar(255) NOT NULL,
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  KEY `idx_tickets_status_prio` (`status`,`priority`),
  KEY `idx_tickets_requester` (`requester_email`(20)),
  KEY `fk_tickets_assignee` (`assignee_id`),
  CONSTRAINT `fk_tickets_assignee` FOREIGN KEY (`assignee_id`) REFERENCES `agents` (`id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=latin1 COLLATE=latin1_swedish_ci;

CREATE TABLE `ticket_legacy_tags` (
  `ticket_id` bigint unsigned NOT NULL,
  `tag` varchar(50) NOT NULL,
  PRIMARY KEY (`ticket_id`,`tag`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1 COMMENT='comma-split tags from the PHP app';

INSERT INTO `agents` (`username`, `email`) VALUES ('bob', 'bob@helpdesk.example'), ('carol', 'carol@helpdesk.example');
INSERT INTO `tickets` (`subject`, `body`, `priority`, `assignee_id`, `requester_email`) VALUES
  ('Printer on fire', 'It is, in fact, on fire. Please advise.', 1, 1, 'dave@customer.example'),
  ('Password reset not arriving and also a very long subject line that goes on and on because the customer pasted their whole email into the subject field again', NULL, 3, NULL, 'erin@customer.example');
INSERT INTO `ticket_legacy_tags` VALUES (1, 'hardware'), (1, 'urgent'), (2, 'auth');
