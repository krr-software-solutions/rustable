CREATE TABLE check_ins (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    habit_id   INTEGER NOT NULL REFERENCES habits (id) ON DELETE CASCADE,
    day        TEXT    NOT NULL CHECK (day = date(day)),
    note       TEXT,
    logged_at  TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (habit_id, day)
);

-- Push-notification reminders (later removed, see the last migration).
CREATE TABLE habit_reminders (
    habit_id INTEGER NOT NULL REFERENCES habits (id) ON DELETE CASCADE,
    at_time  TEXT    NOT NULL CHECK (at_time GLOB '[0-2][0-9]:[0-5][0-9]'),
    PRIMARY KEY (habit_id, at_time)
) WITHOUT ROWID;

INSERT INTO habits (name, frequency) VALUES ('Drink water', 'daily'), ('Long run', 'weekly');
INSERT INTO check_ins (habit_id, day) VALUES (1, '2025-06-01'), (1, '2025-06-02'), (2, '2025-06-01');
INSERT INTO habit_reminders (habit_id, at_time) VALUES (1, '09:00'), (1, '15:00');

PRAGMA user_version = 2;
