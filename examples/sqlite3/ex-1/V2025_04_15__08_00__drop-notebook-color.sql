-- Notebook colors were replaced by emoji icons in the UI. Keep the emoji,
-- seeded from the old color so nothing looks blank, then drop the color.
ALTER TABLE notebooks ADD COLUMN icon TEXT NOT NULL DEFAULT '📓';

UPDATE notebooks
   SET icon = CASE lower(color)
                  WHEN '#3366ff' THEN '📘'
                  WHEN '#ff3333' THEN '📕'
                  WHEN '#33cc33' THEN '📗'
                  ELSE '📓'
              END
 WHERE color IS NOT NULL;

ALTER TABLE notebooks DROP COLUMN color;

PRAGMA user_version = 5;
