SELECT c.id AS "id!", c.payload AS "payload!"
FROM comments c JOIN coverage v ON v.id = c.coverage
WHERE v.published = 1 AND v.channel = ?1 AND c.time >= ?2 AND c.time < ?3
ORDER BY c.time, c.id
LIMIT ?4
