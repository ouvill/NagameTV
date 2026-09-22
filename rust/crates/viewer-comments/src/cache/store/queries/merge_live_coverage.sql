SELECT MIN(start) AS "start!: i64", MAX(end) AS "end!: i64"
FROM (
    SELECT start, end FROM live_coverage
    WHERE owner = ?1 AND channel = ?2 AND clock = ?3 AND start <= ?4 AND end >= ?5
    UNION ALL SELECT ?5, ?4
)
