INSERT INTO targets(key, channel, start, end, last_used) VALUES(?1, ?2, ?3, ?4, ?5)
ON CONFLICT(key) DO UPDATE
SET start = excluded.start, end = excluded.end, last_used = excluded.last_used
