WITH old AS (
    SELECT c.time, c.payload,
           ROW_NUMBER() OVER(PARTITION BY c.time, c.payload ORDER BY c.id) AS occurrence
    FROM comments c JOIN coverage v ON v.id = c.coverage
    WHERE v.published = 1 AND v.channel = ?1 AND c.time >= ?2 AND c.time < ?3
), incoming AS (
    SELECT time, payload,
           ROW_NUMBER() OVER(PARTITION BY time, payload ORDER BY id) AS occurrence
    FROM comments WHERE coverage = ?4
)
INSERT INTO comments(coverage, time, payload)
SELECT ?4, old.time, old.payload
FROM old LEFT JOIN incoming
    ON old.time = incoming.time AND old.payload = incoming.payload
       AND old.occurrence = incoming.occurrence
WHERE incoming.occurrence IS NULL
