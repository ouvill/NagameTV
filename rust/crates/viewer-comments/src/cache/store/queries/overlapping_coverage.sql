SELECT id, start, end, fetched, last_used, settled AS "settled: bool", target_key
FROM coverage
WHERE published = 1 AND channel = ?1 AND start < ?2 AND end > ?3 AND id <> ?4
