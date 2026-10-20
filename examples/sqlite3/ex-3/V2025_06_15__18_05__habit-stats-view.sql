CREATE VIEW v_habit_stats AS
SELECT h.id,
       h.name,
       h.frequency,
       count(c.id)  AS total_check_ins,
       max(c.day)   AS last_day,
       count(CASE WHEN c.day >= date('now', '-6 days') THEN 1 END) AS last_7_days
  FROM habits h
  LEFT JOIN check_ins c ON c.habit_id = h.id
 WHERE h.archived_at IS NULL
 GROUP BY h.id;

PRAGMA user_version = 4;
