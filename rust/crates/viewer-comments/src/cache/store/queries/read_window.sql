SELECT id AS "id!", payload AS "payload!", own AS "own!: bool",
       origin AS "origin!: bool", media_ms AS "media_ms?: i64"
FROM (
    SELECT c.id, c.time, c.payload, 0 AS own, 0 AS origin, NULL AS media_ms
    FROM comments c JOIN coverage v ON v.id = c.coverage
    WHERE v.published = 1 AND v.channel = ?1 AND c.time >= ?2 AND c.time < ?3
    UNION ALL
    SELECT id, time, payload, own, 1 AS origin, media_ms
    FROM live_comments
    WHERE owner = ?4 AND channel = ?1 AND clock = ?5 AND time >= ?2 AND time < ?3
)
ORDER BY time, origin DESC, id
LIMIT ?6
