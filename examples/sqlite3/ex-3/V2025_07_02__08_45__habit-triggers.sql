-- ADD COLUMN cannot use a non-constant default, so updated_at starts NULL and
-- a trigger maintains it from here on.
ALTER TABLE habits ADD COLUMN updated_at TEXT;
UPDATE habits SET updated_at = created_at;

CREATE TRIGGER habits_touch_updated_at
AFTER UPDATE OF name, frequency, archived_at ON habits
FOR EACH ROW
WHEN new.updated_at IS old.updated_at
BEGIN
    UPDATE habits
       SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
     WHERE id = new.id;
END;

-- Refuse check-ins against archived habits.
CREATE TRIGGER check_ins_reject_archived
BEFORE INSERT ON check_ins
FOR EACH ROW
WHEN (SELECT archived_at FROM habits WHERE id = new.habit_id) IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'cannot check in to an archived habit');
END;

PRAGMA user_version = 5;
