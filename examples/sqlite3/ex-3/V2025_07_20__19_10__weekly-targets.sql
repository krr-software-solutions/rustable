-- Replace the daily/weekly enum with a numeric weekly target.
-- Data migration: daily -> 7, weekly -> 1. `frequency` stays for old clients
-- (it is CHECK-constrained, so dropping it would need a table rebuild).
ALTER TABLE habits ADD COLUMN target_per_week INTEGER NOT NULL DEFAULT 7
    CHECK (target_per_week BETWEEN 1 AND 7);

UPDATE habits
   SET target_per_week = CASE frequency WHEN 'weekly' THEN 1 ELSE 7 END;

PRAGMA user_version = 6;
