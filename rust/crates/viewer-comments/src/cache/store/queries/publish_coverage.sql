UPDATE coverage
SET published = 1, settled = ?2, target_key = ?3,
    bytes = ?5 + COALESCE((SELECT SUM(length(payload) + ?4) FROM comments WHERE coverage = ?1), 0)
WHERE id = ?1
