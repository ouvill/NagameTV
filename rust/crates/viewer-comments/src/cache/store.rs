use super::*;
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Duration,
};

const SCHEMA_VERSION: i64 = 1;
const APPLICATION_ID: i64 = 0x4e434d54;
const ROW_CHARGE: i64 = 128;
#[cfg(test)]
mod tests;
pub(super) const IMPORT_BATCH: usize = 512;

pub(super) struct Store {
    db: Connection,
    directory: PathBuf,
}
pub(super) struct ProviderLease {
    _file: File,
    directory: PathBuf,
}
pub(super) struct SessionLease {
    pub name: String,
    _file: File,
    path: PathBuf,
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) enum Reservation<'a> {
    Ready,
    Waiting(i64),
    Granted(EligibleRequest<'a>),
}
/// The persisted request slot, target and provider lock travel together. This
/// non-cloneable value can prepare exactly one response file for that target.
pub(super) struct EligibleRequest<'a> {
    lease: &'a ProviderLease,
    channel: u16,
    range: Interval,
    fetched: i64,
    generation: i64,
}
impl EligibleRequest<'_> {
    pub fn begin(self, id: String) -> Result<super::spool::Receiving, Error> {
        super::spool::Receiving::prepare(
            &self.lease.directory,
            super::spool::Receipt {
                id,
                channel: self.channel,
                range: self.range,
                fetched: self.fetched,
                generation: self.generation,
            },
        )
    }
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self, Error> {
        fs::create_dir_all(directory)?;
        fs::create_dir_all(directory.join("sessions"))?;
        let db = Connection::open(directory.join("cache.sqlite3"))?;
        db.busy_timeout(Duration::from_secs(2))?;
        if db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? == 0 {
            db.execute_batch("PRAGMA auto_vacuum=INCREMENTAL;")?;
        }
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=FILE;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let app: i64 = db.query_row("PRAGMA application_id", [], |row| row.get(0))?;
        if (version != 0 && version != SCHEMA_VERSION) || (app != 0 && app != APPLICATION_ID) {
            return Err(Error::Format("unsupported database version".into()));
        }
        db.execute_batch(include_str!("schema.sql"))?;
        db.pragma_update(None, "application_id", APPLICATION_ID)?;
        db.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self {
            db,
            directory: directory.to_owned(),
        })
    }
    pub fn try_provider(&self) -> Result<Option<ProviderLease>, Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("provider.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(ProviderLease {
                _file: file,
                directory: self.directory.clone(),
            })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }
    pub fn session(&mut self) -> Result<SessionLease, Error> {
        let file = tempfile::Builder::new()
            .prefix("session-")
            .tempfile_in(self.directory.join("sessions"))?;
        let (file, path) = file.keep().map_err(|error| error.error)?;
        file.lock()?;
        let name = path
            .file_name()
            .expect("temporary filename")
            .to_string_lossy()
            .into_owned();
        self.db
            .execute("INSERT INTO sessions(owner) VALUES (?1)", [&name])?;
        Ok(SessionLease {
            name,
            _file: file,
            path,
        })
    }
    pub fn reap_sessions(&mut self) -> Result<(), Error> {
        let names = self
            .db
            .prepare("SELECT owner FROM sessions")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for name in names {
            if !name.starts_with("session-") || name.contains(['/', '\\']) {
                return Err(Error::Format("invalid session owner".into()));
            }
            let path = self.directory.join("sessions").join(&name);
            let dead = match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(file) => match file.try_lock() {
                    Ok(()) => {
                        self.release_session(&name)?;
                        true
                    }
                    Err(std::fs::TryLockError::WouldBlock) => false,
                    Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.release_session(&name)?;
                    true
                }
                Err(error) => return Err(error.into()),
            };
            if dead {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
    pub fn release_session(&mut self, owner: &str) -> Result<(), Error> {
        self.db
            .execute("DELETE FROM sessions WHERE owner=?1", [owner])?;
        Ok(())
    }
    pub fn reset_source(&mut self, owner: &str) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM pins WHERE owner=?1", [owner])?;
        tx.execute("DELETE FROM live_comments WHERE owner=?1", [owner])?;
        tx.execute("DELETE FROM live_coverage WHERE owner=?1", [owner])?;
        tx.commit()?;
        Ok(())
    }
    pub fn pin(&mut self, owner: &str, spans: &[ClockSpan]) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM pins WHERE owner=?1", [owner])?;
        for span in spans {
            if let Some(range) = span.interval() {
                tx.execute(
                    "INSERT INTO pins(owner,channel,start,end) VALUES(?1,?2,?3,?4)",
                    params![owner, span.channel, range.start, range.end],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn generation(&self) -> Result<i64, Error> {
        Ok(self
            .db
            .query_row("SELECT generation FROM provider WHERE id=1", [], |r| {
                r.get(0)
            })?)
    }
    pub fn reserve<'a>(
        &mut self,
        lease: &'a ProviderLease,
        channel: u16,
        range: Interval,
        now: i64,
    ) -> Result<Reservation<'a>, Error> {
        if lease.directory != self.directory {
            return Err(Error::Format(
                "provider lock belongs to another cache".into(),
            ));
        }
        if range.missing(self.covered(channel, range, now)?).is_empty() {
            return Ok(Reservation::Ready);
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (wait, observed, generation): (i64, i64, i64) = tx.query_row(
            "SELECT wait_until,observed,generation FROM provider WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        // A backwards wall clock must never replenish request slots after restart.
        if observed > now {
            return Ok(Reservation::Waiting(observed.max(wait)));
        }
        tx.execute("UPDATE provider SET observed=?1 WHERE id=1", [now])?;
        tx.execute(
            "DELETE FROM requests WHERE started<=?1",
            [now - REQUEST_WINDOW_SECONDS],
        )?;
        let starts = tx
            .prepare("SELECT started FROM requests ORDER BY started")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut until = wait;
        if let Some(last) = starts.last() {
            until = until.max(last + REQUEST_SPACING_SECONDS);
        }
        if starts.len() >= REQUESTS_PER_WINDOW {
            until = until.max(starts[starts.len() - REQUESTS_PER_WINDOW] + REQUEST_WINDOW_SECONDS);
        }
        if until > now {
            tx.commit()?;
            return Ok(Reservation::Waiting(until));
        }
        tx.execute("INSERT INTO requests(started) VALUES(?1)", [now])?;
        tx.commit()?;
        Ok(Reservation::Granted(EligibleRequest {
            lease,
            channel,
            range,
            fetched: now,
            generation,
        }))
    }
    pub fn failed(
        &mut self,
        permanent: bool,
        retry_after: Option<i64>,
        now: i64,
    ) -> Result<(), Error> {
        let failures: u32 =
            self.db
                .query_row("SELECT failures FROM provider WHERE id=1", [], |r| r.get(0))?;
        let failures = failures.saturating_add(1).min(16);
        let delay = if permanent {
            30 * 60
        } else {
            (5_i64 * 60 << (failures - 1).min(3)).min(30 * 60)
        };
        use std::hash::BuildHasher;
        let jitter =
            (std::collections::hash_map::RandomState::new().hash_one((now, failures)) % 31) as i64;
        let until = (now + delay + jitter).max(retry_after.unwrap_or(0));
        self.db.execute(
            "UPDATE provider SET failures=?1,wait_until=MAX(wait_until,?2) WHERE id=1",
            params![failures, until],
        )?;
        Ok(())
    }
    pub fn covered(
        &self,
        channel: u16,
        wanted: Interval,
        now: i64,
    ) -> Result<Vec<Interval>, Error> {
        Ok(self.db.prepare("SELECT start,end FROM coverage WHERE published=1 AND channel=?1 AND start<?2 AND end>?3
            AND (fetched>=end+?4 OR fetched>?5) ORDER BY start")?
            .query_map(params![channel,wanted.end,wanted.start,SETTLED_SECONDS,now-RECHECK_SECONDS], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?
            .map(|row| { let (a,b)=row?; Interval::new(a,b).ok_or_else(|| Error::Format("invalid coverage".into())) }).collect::<Result<Vec<_>,Error>>()?)
    }
    pub fn live_covered(
        &self,
        owner: &str,
        channel: u16,
        key: &str,
        wanted: Interval,
    ) -> Result<Vec<Interval>, Error> {
        self.db.prepare("SELECT start,end FROM live_coverage WHERE owner=?1 AND channel=?2 AND clock=?3 AND start<?4 AND end>?5")?
            .query_map(params![owner,channel,key,wanted.end,wanted.start], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?
            .map(|row| { let (a,b)=row?; Interval::new(a,b).ok_or_else(|| Error::Format("invalid live coverage".into())) }).collect()
    }
    pub fn observe_live(
        &mut self,
        owner: &str,
        channel: u16,
        key: &str,
        range: Interval,
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        let (a,b): (i64,i64) = tx.query_row("SELECT MIN(start),MAX(end) FROM (
            SELECT start,end FROM live_coverage WHERE owner=?1 AND channel=?2 AND clock=?3 AND start<=?4 AND end>=?5
            UNION ALL SELECT ?5,?4)", params![owner,channel,key,range.end,range.start], |r| Ok((r.get(0)?,r.get(1)?)))?;
        tx.execute("DELETE FROM live_coverage WHERE owner=?1 AND channel=?2 AND clock=?3 AND start<=?4 AND end>=?5", params![owner,channel,key,b,a])?;
        tx.execute(
            "INSERT INTO live_coverage(owner,channel,clock,start,end) VALUES(?1,?2,?3,?4,?5)",
            params![owner, channel, key, a, b],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn insert_live(
        &mut self,
        owner: &str,
        channel: u16,
        clock: Option<&ClockSpan>,
        comments: &[(Comment, bool)],
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        {
            let mut insert = tx.prepare("INSERT INTO live_comments(owner,channel,clock,media_ms,time,payload,own) VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
            for (comment, own) in comments {
                let Some(time) = comment.timestamp_micros.and_then(|t| i64::try_from(t).ok())
                else {
                    continue;
                };
                let payload = serde_json::to_vec(&StoredComment::from(comment.clone()))
                    .map_err(|e| Error::Format(e.to_string()))?;
                insert.execute(params![
                    owner,
                    channel,
                    clock.map(|c| c.key.as_str()),
                    clock.and_then(|c| c.media_ms(time as u64)),
                    time,
                    payload,
                    own
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn retain_live(
        &mut self,
        owner: &str,
        earliest_ms: i64,
        spans: &[ClockSpan],
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        for span in spans {
            let end = span
                .utc_start_ms
                .saturating_add(span.media_end_ms - span.media_start_ms);
            // Associate reception before the first clock only where the current
            // TS map has one possible position. Overlapping UTC epochs remain
            // unassigned and protected until their owning TS session ends.
            if let Some(range) = span.interval() {
                let others = spans
                    .iter()
                    .filter(|other| other.key != span.key)
                    .filter_map(ClockSpan::interval);
                for unique in range.missing(others) {
                    tx.execute("UPDATE live_comments SET clock=?1 WHERE owner=?2 AND channel=?3 AND clock IS NULL AND time>=?4 AND time<?5",
                        params![span.key,owner,span.channel,unique.start*1_000_000,unique.end*1_000_000])?;
                }
            }
            // A known clock key also protects comments ahead of received video.
            tx.execute(
                "UPDATE live_comments SET media_ms=?1+(time/1000-?2) WHERE owner=?3 AND clock=?4
                AND media_ms IS NULL AND time>=?2*1000 AND time<?5*1000",
                params![span.media_start_ms, span.utc_start_ms, owner, span.key, end],
            )?;
        }
        tx.execute(
            "DELETE FROM live_comments WHERE owner=?1 AND media_ms<?2",
            params![owner, earliest_ms.saturating_sub(LOOKBACK_SECONDS * 1000)],
        )?;
        // Do not remove an unmapped comment by its UTC age. A later clock or the
        // end of its TS-owning session decides its lifetime.
        tx.commit()?;
        Ok(())
    }
    pub fn begin_import(&mut self, receipt: &spool::Receipt) -> Result<i64, Error> {
        let tx = self.db.transaction()?;
        let existing: Option<(i64, bool)> = tx
            .query_row(
                "SELECT id,published FROM coverage WHERE receipt=?1",
                [&receipt.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, true)) = existing {
            return Ok(id);
        }
        if let Some((id, false)) = existing {
            tx.execute("DELETE FROM coverage WHERE id=?1", [id])?;
        }
        tx.execute("INSERT INTO coverage(channel,start,end,fetched,last_used,receipt,published,bytes) VALUES(?1,?2,?3,?4,?4,?5,0,0)",
            params![receipt.channel,receipt.range.start,receipt.range.end,receipt.fetched,receipt.id])?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }
    pub fn imported(&self, receipt: &str) -> Result<bool, Error> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM coverage WHERE receipt=?1 AND published=1)",
            [receipt],
            |r| r.get(0),
        )?)
    }
    pub fn can_publish(&self, receipt: &spool::Receipt) -> Result<bool, Error> {
        Ok(self.generation()? == receipt.generation
            || self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM pins WHERE channel=?1 AND start<?2 AND end>?3)",
                params![receipt.channel, receipt.range.end, receipt.range.start],
                |r| r.get::<_, bool>(0),
            )?)
    }
    pub fn discard_staged(
        &mut self,
        _lease: &ProviderLease,
        keep: Option<&str>,
    ) -> Result<(), Error> {
        self.db.execute(
            "DELETE FROM coverage WHERE published=0 AND (?1 IS NULL OR receipt<>?1)",
            [keep],
        )?;
        Ok(())
    }
    pub fn import_batch(
        &mut self,
        id: i64,
        range: Interval,
        comments: &mut Vec<Comment>,
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        let mut bytes = 0;
        {
            let mut insert =
                tx.prepare("INSERT INTO comments(coverage,time,payload) VALUES(?1,?2,?3)")?;
            for comment in comments.iter() {
                let Some(time) = comment
                    .timestamp_micros
                    .and_then(|t| i64::try_from(t).ok())
                    .filter(|t| range.contains(t / 1_000_000))
                else {
                    continue;
                };
                let payload = serde_json::to_vec(&StoredComment::from(comment.clone()))
                    .map_err(|e| Error::Format(e.to_string()))?;
                bytes += payload.len() as i64 + ROW_CHARGE;
                insert.execute(params![id, time, payload])?;
            }
        }
        tx.execute(
            "UPDATE coverage SET bytes=bytes+?1 WHERE id=?2",
            params![bytes, id],
        )?;
        tx.commit()?;
        comments.clear();
        Ok(())
    }
    pub fn publish(&mut self, id: i64, receipt: &spool::Receipt) -> Result<(), Error> {
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let generation: i64 =
            tx.query_row("SELECT generation FROM provider WHERE id=1", [], |r| {
                r.get(0)
            })?;
        let pinned: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pins WHERE channel=?1 AND start<?2 AND end>?3)",
            params![receipt.channel, receipt.range.end, receipt.range.start],
            |r| r.get(0),
        )?;
        if receipt.generation != generation && !pinned {
            tx.execute("DELETE FROM coverage WHERE id=?1", [id])?;
            tx.commit()?;
            return Ok(());
        }
        let range = receipt.range;
        let overlap=tx.prepare("SELECT id,start,end,fetched,last_used FROM coverage WHERE published=1 AND channel=?1 AND start<?2 AND end>?3 AND id<>?4")?
            .query_map(params![receipt.channel,range.end,range.start,id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?)))?.collect::<Result<Vec<_>,_>>()?;
        for (old, start, end, fetched, used) in overlap {
            for remain in [
                Interval::new(start, range.start.min(end)),
                Interval::new(range.end.max(start), end),
            ]
            .into_iter()
            .flatten()
            {
                tx.execute("INSERT INTO coverage(channel,start,end,fetched,last_used,published,bytes) VALUES(?1,?2,?3,?4,?5,1,0)",params![receipt.channel,remain.start,remain.end,fetched,used])?;
                let kept = tx.last_insert_rowid();
                tx.execute(
                    "UPDATE comments SET coverage=?1 WHERE coverage=?2 AND time>=?3 AND time<?4",
                    params![kept, old, remain.start * 1_000_000, remain.end * 1_000_000],
                )?;
                tx.execute("UPDATE coverage SET bytes=COALESCE((SELECT SUM(length(payload)+?1) FROM comments WHERE coverage=?2),0) WHERE id=?2",params![ROW_CHARGE,kept])?;
            }
            tx.execute("DELETE FROM coverage WHERE id=?1", [old])?;
        }
        tx.execute("UPDATE coverage SET published=1 WHERE id=?1", [id])?;
        tx.execute(
            "UPDATE provider SET failures=0,wait_until=0,revision=revision+1 WHERE id=1",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn revision(&self) -> Result<i64, Error> {
        Ok(self
            .db
            .query_row("SELECT revision FROM provider WHERE id=1", [], |r| r.get(0))?)
    }
    pub fn read(
        &mut self,
        owner: &str,
        channel: u16,
        view: &View,
        now: i64,
    ) -> Result<Vec<Record>, Error> {
        let mut statement=self.db.prepare("SELECT id,time,payload,own,origin,media_ms FROM (
            SELECT c.id,c.time,c.payload,0 AS own,0 AS origin,NULL AS media_ms FROM comments c JOIN coverage v ON v.id=c.coverage
                WHERE v.published=1 AND v.channel=?1 AND c.time>=?2 AND c.time<?3
            UNION ALL SELECT id,time,payload,own,1 AS origin,media_ms FROM live_comments
                WHERE owner=?4 AND channel=?1 AND clock=?5 AND time>=?2 AND time<?3)
            ORDER BY time,origin DESC,id LIMIT ?6")?;
        let mut rows = statement.query(params![
            channel,
            view.interval.start * 1_000_000,
            view.interval.end * 1_000_000,
            owner,
            view.clock_key,
            WORKING_COMMENTS as i64
        ])?;
        let mut records = Vec::new();
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let payload: Vec<u8> = row.get(2)?;
            bytes += payload.len() + std::mem::size_of::<Record>();
            if bytes > WORKING_BYTES {
                break;
            }
            let stored: StoredComment =
                serde_json::from_slice(&payload).map_err(|e| Error::Format(e.to_string()))?;
            let id: i64 = row.get(0)?;
            let live: bool = row.get(4)?;
            records.push(Record {
                id: (id as u64) * 2 + u64::from(live),
                comment: stored.into(),
                own: row.get(3)?,
                origin: if live {
                    RecordOrigin::Live
                } else {
                    RecordOrigin::Archive
                },
                media_ms: row.get(5)?,
            });
        }
        drop(rows);
        drop(statement);
        self.db.execute("UPDATE coverage SET last_used=?1 WHERE published=1 AND channel=?2 AND start<?3 AND end>?4 AND last_used<?1",params![now,channel,view.interval.end,view.interval.start])?;
        Ok(records)
    }
    pub fn cleanup(&mut self, now: i64, clear: bool) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        if clear {
            tx.execute(
                "UPDATE provider SET generation=generation+1,revision=revision+1 WHERE id=1",
                [],
            )?;
        }
        let candidates=tx.prepare("SELECT id,bytes,last_used FROM coverage v WHERE published=1 AND NOT EXISTS
            (SELECT 1 FROM pins p WHERE p.channel=v.channel AND p.start<v.end AND p.end>v.start) ORDER BY last_used,id")?
            .query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?)))?.collect::<Result<Vec<_>,_>>()?;
        let mut count = candidates.len() as i64;
        let mut bytes = candidates.iter().map(|c| c.1).sum::<i64>();
        for (id, charge, used) in candidates {
            if clear
                || bytes > CACHE_BYTES
                || count > CACHE_INTERVALS
                || used < now - CACHE_AGE_SECONDS
            {
                tx.execute("DELETE FROM coverage WHERE id=?1", [id])?;
                bytes -= charge;
                count -= 1;
            }
        }
        tx.commit()?;
        self.db
            .execute_batch("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
    pub fn disk_bytes(&self) -> u64 {
        [
            "cache.sqlite3",
            "cache.sqlite3-wal",
            "response.part",
            "response.json",
        ]
        .iter()
        .filter_map(|name| fs::metadata(self.directory.join(name)).ok())
        .map(|m| m.len())
        .sum()
    }
}
