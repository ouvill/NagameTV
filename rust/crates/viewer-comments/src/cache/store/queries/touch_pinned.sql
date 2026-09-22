UPDATE coverage
SET last_used = ?1
WHERE published = 1 AND last_used < ?1 AND EXISTS (
    SELECT 1 FROM pins p
    WHERE p.channel = coverage.channel AND p.start < coverage.end AND p.end > coverage.start
)
