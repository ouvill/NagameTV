SELECT COALESCE(v.target_key, 'legacy:' || v.id) AS "key!: String",
       SUM(v.bytes) AS "bytes!: i64"
FROM coverage v
WHERE v.published = 1
GROUP BY COALESCE(v.target_key, 'legacy:' || v.id)
HAVING NOT EXISTS (
    SELECT 1 FROM coverage member JOIN pins p
        ON p.channel = member.channel AND p.start < member.end AND p.end > member.start
    WHERE member.published = 1 AND (member.target_key = v.target_key OR member.id = v.id)
)
ORDER BY MAX(v.last_used), MIN(v.id)
