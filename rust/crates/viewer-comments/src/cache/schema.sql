CREATE TABLE IF NOT EXISTS provider (
    id INTEGER PRIMARY KEY CHECK(id=1), wait_until INTEGER NOT NULL DEFAULT 0,
    failures INTEGER NOT NULL DEFAULT 0, observed INTEGER NOT NULL DEFAULT 0,
    generation INTEGER NOT NULL DEFAULT 0, revision INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO provider(id) VALUES(1);
CREATE TABLE IF NOT EXISTS requests (started INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS request_time ON requests(started);
CREATE TABLE IF NOT EXISTS coverage (
    id INTEGER PRIMARY KEY AUTOINCREMENT, channel INTEGER NOT NULL,
    start INTEGER NOT NULL, end INTEGER NOT NULL CHECK(end>start),
    fetched INTEGER NOT NULL, last_used INTEGER NOT NULL,
    receipt TEXT UNIQUE, published INTEGER NOT NULL CHECK(published IN (0,1)), bytes INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS coverage_time ON coverage(channel,start,end);
CREATE TABLE IF NOT EXISTS comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    coverage INTEGER NOT NULL REFERENCES coverage(id) ON DELETE CASCADE,
    time INTEGER NOT NULL, payload BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS comment_time ON comments(coverage,time);
CREATE TABLE IF NOT EXISTS sessions (owner TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS pins (
    owner TEXT NOT NULL REFERENCES sessions(owner) ON DELETE CASCADE,
    channel INTEGER NOT NULL, start INTEGER NOT NULL, end INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS pin_time ON pins(channel,start,end);
CREATE TABLE IF NOT EXISTS live_comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    owner TEXT NOT NULL REFERENCES sessions(owner) ON DELETE CASCADE,
    channel INTEGER NOT NULL, clock TEXT, media_ms INTEGER, time INTEGER NOT NULL,
    payload BLOB NOT NULL, own INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS live_comment_time ON live_comments(owner,channel,time);
CREATE INDEX IF NOT EXISTS live_comment_retention ON live_comments(owner,media_ms);
CREATE INDEX IF NOT EXISTS live_unmapped_clock ON live_comments(owner,clock,time) WHERE media_ms IS NULL;
CREATE INDEX IF NOT EXISTS live_unknown_clock ON live_comments(owner,channel,time) WHERE clock IS NULL;
CREATE TABLE IF NOT EXISTS live_coverage (
    owner TEXT NOT NULL REFERENCES sessions(owner) ON DELETE CASCADE,
    channel INTEGER NOT NULL, clock TEXT NOT NULL, start INTEGER NOT NULL, end INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS live_coverage_time ON live_coverage(owner,channel,clock,start);
CREATE TABLE IF NOT EXISTS targets (
    key TEXT PRIMARY KEY, channel INTEGER NOT NULL,
    start INTEGER NOT NULL, end INTEGER NOT NULL CHECK(end>start),
    last_used INTEGER NOT NULL,
    refresh INTEGER NOT NULL DEFAULT 0, completed_refresh INTEGER NOT NULL DEFAULT 0,
    failures INTEGER NOT NULL DEFAULT 0, retry_at INTEGER NOT NULL DEFAULT 0,
    failure TEXT, stopped INTEGER NOT NULL DEFAULT 0 CHECK(stopped IN (0,1))
);
