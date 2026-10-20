-- SHOP-127: phone -> phone_number, and tighten to 20 chars (E.164 max is 15 + formatting).
-- !!! BREAKING: old app pods still SELECT `phone`. Deploy app v2.8+ first. !!!

-- truncate anything that wouldn't fit (there were 3 rows with junk like "call after 5pm pls")
UPDATE `customers`
   SET `phone` = LEFT(REPLACE(`phone`, ' ', ''), 20)
 WHERE CHAR_LENGTH(`phone`) > 20 OR `phone` LIKE '% %';

ALTER TABLE `customers`
  CHANGE COLUMN `phone` `phone_number` varchar(20) COLLATE utf8mb4_unicode_ci DEFAULT NULL COMMENT 'E.164-ish, no spaces',
  ALGORITHM=COPY;
