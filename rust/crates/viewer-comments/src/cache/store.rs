use super::*;
use futures_lite::{StreamExt, future::block_on};
use sqlx::{Connection, SqliteConnection};
mod database;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

const ROW_CHARGE: i64 = 128;
const COVERAGE_CHARGE: i64 = 512;
#[cfg(test)]
mod tests;
pub(super) const IMPORT_BATCH: usize = 512;

pub(super) struct Store {
    db: SqliteConnection,
    directory: PathBuf,
    cache_bytes: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct RequestPlan {
    pub target: plan::Target,
    pub range: Interval,
    pub refresh: i64,
}
pub(super) enum Planned {
    Complete,
    Ready(RequestPlan),
    Waiting(i64),
    Failed(String),
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
    plan: Option<RequestPlan>,
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
                plan: self.plan,
            },
        )
    }
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self, Error> {
        fs::create_dir_all(directory)?;
        fs::create_dir_all(directory.join("sessions"))?;
        let db = database::open(&directory.join("cache.sqlite3"))?;
        Ok(Self {
            db,
            directory: directory.to_owned(),
            cache_bytes: DEFAULT_CACHE_BYTES,
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
        block_on(
            sqlx::query!("INSERT INTO sessions(owner) VALUES (?1)", &name).execute(&mut self.db),
        )?;
        Ok(SessionLease {
            name,
            _file: file,
            path,
        })
    }
    pub fn reap_sessions(&mut self) -> Result<(), Error> {
        let names =
            block_on(sqlx::query_scalar!("SELECT owner FROM sessions").fetch_all(&mut self.db))?;
        for name in names {
            let name = name.ok_or_else(|| Error::Format("null session owner".into()))?;
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
        block_on(sqlx::query!("DELETE FROM sessions WHERE owner=?1", owner).execute(&mut self.db))?;
        Ok(())
    }
    pub fn reset_source(&mut self, owner: &str) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
        block_on(sqlx::query!("DELETE FROM pins WHERE owner=?1", owner).execute(&mut *tx))?;
        block_on(
            sqlx::query!("DELETE FROM live_comments WHERE owner=?1", owner).execute(&mut *tx),
        )?;
        block_on(
            sqlx::query!("DELETE FROM live_coverage WHERE owner=?1", owner).execute(&mut *tx),
        )?;
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn pin(&mut self, owner: &str, spans: &[ClockSpan], now: i64) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
        block_on(sqlx::query!("DELETE FROM pins WHERE owner=?1", owner).execute(&mut *tx))?;
        for span in spans {
            if let Some(range) = span.interval() {
                block_on(
                    sqlx::query!(
                        "INSERT INTO pins(owner,channel,start,end) VALUES(?1,?2,?3,?4)",
                        owner,
                        span.channel,
                        range.start,
                        range.end
                    )
                    .execute(&mut *tx),
                )?;
            }
        }
        touch_pinned(&mut tx, now)?;
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn generation(&mut self) -> Result<i64, Error> {
        Ok(block_on(
            sqlx::query_scalar!("SELECT generation FROM provider WHERE id=1")
                .fetch_one(&mut self.db),
        )?)
    }
    #[cfg(test)]
    pub fn reserve<'a>(
        &mut self,
        lease: &'a ProviderLease,
        channel: u16,
        range: Interval,
        now: i64,
    ) -> Result<Reservation<'a>, Error> {
        self.reserve_slot(lease, channel, range, now, None)
    }
    pub fn reserve_plan<'a>(
        &mut self,
        lease: &'a ProviderLease,
        request: RequestPlan,
        now: i64,
    ) -> Result<Reservation<'a>, Error> {
        self.register_target(&request.target, now)?;
        self.reserve_slot(
            lease,
            request.target.channel,
            request.range,
            now,
            Some(request),
        )
    }
    fn reserve_slot<'a>(
        &mut self,
        lease: &'a ProviderLease,
        channel: u16,
        range: Interval,
        now: i64,
        plan: Option<RequestPlan>,
    ) -> Result<Reservation<'a>, Error> {
        if lease.directory != self.directory {
            return Err(Error::Format(
                "provider lock belongs to another cache".into(),
            ));
        }
        if plan.is_none() && range.missing(self.covered(channel, range, now)?).is_empty() {
            return Ok(Reservation::Ready);
        }
        let mut tx = block_on(self.db.begin_with("BEGIN IMMEDIATE"))?;
        let provider = block_on(
            sqlx::query!("SELECT wait_until,observed,generation FROM provider WHERE id=1")
                .fetch_one(&mut *tx),
        )?;
        let (wait, observed, generation) =
            (provider.wait_until, provider.observed, provider.generation);
        // A backwards wall clock must never replenish request slots after restart.
        if observed > now {
            return Ok(Reservation::Waiting(observed.max(wait)));
        }
        block_on(
            sqlx::query!("UPDATE provider SET observed=?1 WHERE id=1", now).execute(&mut *tx),
        )?;
        block_on(
            sqlx::query!(
                "DELETE FROM requests WHERE started<=?1",
                now - REQUEST_WINDOW_SECONDS
            )
            .execute(&mut *tx),
        )?;
        let starts = block_on(
            sqlx::query_scalar!("SELECT started FROM requests ORDER BY started")
                .fetch_all(&mut *tx),
        )?;
        let mut until = wait;
        if let Some(last) = starts.last() {
            until = until.max(last + REQUEST_SPACING_SECONDS);
        }
        if starts.len() >= REQUESTS_PER_WINDOW {
            until = until.max(starts[starts.len() - REQUESTS_PER_WINDOW] + REQUEST_WINDOW_SECONDS);
        }
        if until > now {
            block_on(tx.commit())?;
            return Ok(Reservation::Waiting(until));
        }
        block_on(sqlx::query!("INSERT INTO requests(started) VALUES(?1)", now).execute(&mut *tx))?;
        block_on(tx.commit())?;
        Ok(Reservation::Granted(EligibleRequest {
            lease,
            channel,
            range,
            fetched: now,
            generation,
            plan,
        }))
    }
    pub fn failed(
        &mut self,
        permanent: bool,
        retry_after: Option<i64>,
        now: i64,
    ) -> Result<(), Error> {
        let failures = block_on(
            sqlx::query_scalar!("SELECT failures FROM provider WHERE id=1").fetch_one(&mut self.db),
        )?;
        let failures = u32::try_from(failures)
            .map_err(|_| Error::Format("invalid provider failure count".into()))?;
        let failures = failures.saturating_add(1).min(16);
        let delay = if permanent {
            30 * 60
        } else {
            ((5_i64 * 60) << (failures - 1).min(3)).min(30 * 60)
        };
        use std::hash::BuildHasher;
        let jitter =
            (std::collections::hash_map::RandomState::new().hash_one((now, failures)) % 31) as i64;
        let until = (now + delay + jitter).max(retry_after.unwrap_or(0));
        block_on(
            sqlx::query!(
                "UPDATE provider SET failures=?1,wait_until=MAX(wait_until,?2) WHERE id=1",
                failures,
                until
            )
            .execute(&mut self.db),
        )?;
        Ok(())
    }
    pub fn covered(
        &mut self,
        channel: u16,
        wanted: Interval,
        now: i64,
    ) -> Result<Vec<Interval>, Error> {
        let _ = now; // Successful data never expires for playback or ordinary reuse.
        self.coverage(channel, wanted, false)
    }
    pub fn is_settled(&mut self, channel: u16, range: Interval) -> Result<bool, Error> {
        Ok(range
            .missing(self.coverage(channel, range, true)?)
            .is_empty())
    }
    fn coverage(
        &mut self,
        channel: u16,
        wanted: Interval,
        settled_only: bool,
    ) -> Result<Vec<Interval>, Error> {
        block_on(sqlx::query!("SELECT start,end FROM coverage WHERE published=1 AND channel=?1 AND start<?2 AND end>?3 AND (?4=0 OR settled=1) ORDER BY start", channel, wanted.end, wanted.start, settled_only).fetch_all(&mut self.db))?
            .into_iter().map(|row| Interval::new(row.start, row.end).ok_or_else(|| Error::Format("invalid coverage".into()))).collect()
    }
    pub fn register_target(&mut self, target: &plan::Target, now: i64) -> Result<(), Error> {
        block_on(
            sqlx::query_file!(
                "src/cache/store/queries/register_target.sql",
                target.key(),
                target.channel,
                target.range.start,
                target.range.end,
                now
            )
            .execute(&mut self.db),
        )?;
        Ok(())
    }
    pub fn request_refresh(&mut self, target: &plan::Target, now: i64) -> Result<(), Error> {
        self.register_target(target, now)?;
        let mut tx = block_on(self.db.begin())?;
        block_on(sqlx::query!("UPDATE targets SET refresh=completed_refresh+1,failures=0,stopped=0,failure=NULL WHERE key=?1", target.key()).execute(&mut *tx))?;
        block_on(
            sqlx::query!("UPDATE provider SET revision=revision+1 WHERE id=1").execute(&mut *tx),
        )?;
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn planned(
        &mut self,
        owner: &str,
        target: &plan::Target,
        now: i64,
    ) -> Result<Planned, Error> {
        let state = block_on(sqlx::query!(r#"SELECT refresh,completed_refresh,stopped AS "stopped: bool",retry_at,failure FROM targets WHERE key=?1"#, target.key()).fetch_optional(&mut self.db))?;
        let (refresh, completed, stopped, retry_at, failure) = state
            .map(|row| {
                (
                    row.refresh,
                    row.completed_refresh,
                    row.stopped,
                    row.retry_at,
                    row.failure,
                )
            })
            .unwrap_or((0, 0, false, 0, None));
        if stopped {
            return Ok(Planned::Failed(
                failure.unwrap_or_else(|| "実況の再取得が必要です".into()),
            ));
        }
        if retry_at > now {
            return Ok(Planned::Waiting(retry_at));
        }
        let Some(range) = Interval::new(target.range.start, target.range.end.min(archive_end(now)))
        else {
            return Ok(Planned::Complete);
        };
        let mut covered = if refresh > completed {
            vec![]
        } else {
            self.coverage(
                target.channel,
                range,
                now >= target.range.end.saturating_add(SETTLED_SECONDS),
            )?
        };
        if let Some(clock) = target.live_clock() {
            covered.extend(self.live_covered(owner, target.channel, clock, range)?);
        }
        let gaps = range.missing(covered);
        #[cfg(feature = "network")]
        if !gaps.is_empty() {
            tracing::debug!(target: "comment_archive", target_key = %target.key(),
                manual = refresh > completed, post_broadcast_check = now >= target.range.end.saturating_add(SETTLED_SECONDS),
                gaps = gaps.len(), missing_seconds = gaps.iter().map(|gap| gap.end-gap.start).sum::<i64>(),
                "archive acquisition needed");
        }
        let (Some(first), Some(last)) = (gaps.first(), gaps.last()) else {
            return Ok(Planned::Complete);
        };
        let mut selected = Interval::new(first.start, last.end).expect("gap envelope");
        if selected.end - selected.start == 1 {
            selected = if selected.start > range.start {
                Interval::new(selected.start - 1, selected.end)
            } else {
                Interval::new(selected.start, (selected.end + 1).min(range.end))
            }
            .expect("nonempty range");
        }
        if selected.end - selected.start < 2 {
            return Ok(Planned::Complete);
        }
        Ok(Planned::Ready(RequestPlan {
            target: target.clone(),
            range: selected.request_prefix(),
            refresh,
        }))
    }
    pub fn set_budget(&mut self, bytes: i64) {
        self.cache_bytes = bytes.max(0);
    }
    pub fn pin_target(
        &mut self,
        owner: &str,
        target: &plan::Target,
        now: i64,
    ) -> Result<(), Error> {
        self.register_target(target, now)?;
        self.pin_archive(owner, target.channel, target.range, now)
    }
    pub fn pin_archive(
        &mut self,
        owner: &str,
        channel: u16,
        range: Interval,
        now: i64,
    ) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
        block_on(sqlx::query!("DELETE FROM pins WHERE owner=?1", owner).execute(&mut *tx))?;
        block_on(
            sqlx::query!(
                "INSERT INTO pins(owner,channel,start,end) VALUES(?1,?2,?3,?4)",
                owner,
                channel,
                range.start,
                range.end
            )
            .execute(&mut *tx),
        )?;
        touch_pinned(&mut tx, now)?;
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn fail_target(
        &mut self,
        target: &plan::Target,
        message: &str,
        permanent: bool,
        retry_after: Option<i64>,
        now: i64,
    ) -> Result<Option<i64>, Error> {
        self.register_target(target, now)?;
        self.failed(permanent, retry_after, now)?;
        let failures = block_on(
            sqlx::query_scalar!("SELECT failures FROM targets WHERE key=?1", target.key())
                .fetch_one(&mut self.db),
        )?;
        let failures = failures + 1;
        let stopped = permanent || failures >= 3;
        let until =
            (now + if failures == 1 { 5 * 60 } else { 30 * 60 }).max(retry_after.unwrap_or(0));
        block_on(
            sqlx::query!(
                "UPDATE targets SET failures=?2,retry_at=?3,stopped=?4,failure=?5 WHERE key=?1",
                target.key(),
                failures,
                until,
                stopped,
                message
            )
            .execute(&mut self.db),
        )?;
        Ok((!stopped).then_some(until))
    }

    pub fn live_covered(
        &mut self,
        owner: &str,
        channel: u16,
        key: &str,
        wanted: Interval,
    ) -> Result<Vec<Interval>, Error> {
        block_on(sqlx::query!("SELECT start,end FROM live_coverage WHERE owner=?1 AND channel=?2 AND clock=?3 AND start<?4 AND end>?5", owner, channel, key, wanted.end, wanted.start).fetch_all(&mut self.db))?
            .into_iter().map(|row| Interval::new(row.start, row.end).ok_or_else(|| Error::Format("invalid live coverage".into()))).collect()
    }
    pub fn observe_live(
        &mut self,
        owner: &str,
        channel: u16,
        key: &str,
        range: Interval,
    ) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
        // The UNION includes the requested range, so both aggregates are non-null.
        let merged = block_on(
            sqlx::query_file!(
                "src/cache/store/queries/merge_live_coverage.sql",
                owner,
                channel,
                key,
                range.end,
                range.start
            )
            .fetch_one(&mut *tx),
        )?;
        let (a, b) = (merged.start, merged.end);
        block_on(sqlx::query!("DELETE FROM live_coverage WHERE owner=?1 AND channel=?2 AND clock=?3 AND start<=?4 AND end>=?5", owner, channel, key, b, a).execute(&mut *tx))?;
        block_on(
            sqlx::query!(
                "INSERT INTO live_coverage(owner,channel,clock,start,end) VALUES(?1,?2,?3,?4,?5)",
                owner,
                channel,
                key,
                a,
                b
            )
            .execute(&mut *tx),
        )?;
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn insert_live(
        &mut self,
        owner: &str,
        channel: u16,
        clock: Option<&ClockSpan>,
        comments: &[(Comment, bool)],
    ) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
        {
            for (comment, own) in comments {
                let Some(time) = comment.timestamp_micros.and_then(|t| i64::try_from(t).ok())
                else {
                    continue;
                };
                let payload = serde_json::to_vec(&StoredComment::from(comment.clone()))?;
                block_on(sqlx::query!("INSERT INTO live_comments(owner,channel,clock,media_ms,time,payload,own) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    owner, channel, clock.map(|c| c.key.as_str()), clock.and_then(|c| c.media_ms(time as u64)), time, payload, own).execute(&mut *tx))?;
            }
        }
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn retain_live(
        &mut self,
        owner: &str,
        earliest_ms: i64,
        spans: &[ClockSpan],
    ) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
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
                    block_on(sqlx::query!("UPDATE live_comments SET clock=?1 WHERE owner=?2 AND channel=?3 AND clock IS NULL AND time>=?4 AND time<?5", span.key, owner, span.channel, unique.start*1_000_000, unique.end*1_000_000).execute(&mut *tx))?;
                }
            }
            // A known clock key also protects comments ahead of received video.
            block_on(sqlx::query!("UPDATE live_comments SET media_ms=?1+(time/1000-?2) WHERE owner=?3 AND clock=?4
                AND media_ms IS NULL AND time>=?2*1000 AND time<?5*1000", span.media_start_ms, span.utc_start_ms, owner, span.key, end).execute(&mut *tx))?;
        }
        block_on(
            sqlx::query!(
                "DELETE FROM live_comments WHERE owner=?1 AND media_ms<?2",
                owner,
                earliest_ms.saturating_sub(LOOKBACK_SECONDS * 1000)
            )
            .execute(&mut *tx),
        )?;
        // Do not remove an unmapped comment by its UTC age. A later clock or the
        // end of its TS-owning session decides its lifetime.
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn begin_import(&mut self, receipt: &spool::Receipt) -> Result<i64, Error> {
        if let Some(plan) = &receipt.plan {
            self.register_target(&plan.target, receipt.fetched)?;
        }
        let mut tx = block_on(self.db.begin())?;
        let existing = block_on(
            sqlx::query!(
                r#"SELECT id,published AS "published: bool" FROM coverage WHERE receipt=?1"#,
                receipt.id
            )
            .fetch_optional(&mut *tx),
        )?
        .map(|row| (row.id, row.published));
        if let Some((id, true)) = existing {
            return Ok(id);
        }
        if let Some((id, false)) = existing {
            block_on(sqlx::query!("DELETE FROM coverage WHERE id=?1", id).execute(&mut *tx))?;
        }
        let inserted = block_on(sqlx::query!("INSERT INTO coverage(channel,start,end,fetched,last_used,receipt,published,bytes) VALUES(?1,?2,?3,?4,?4,?5,0,?6)", receipt.channel, receipt.range.start, receipt.range.end, receipt.fetched, receipt.id, COVERAGE_CHARGE).execute(&mut *tx))?;
        let id = inserted.last_insert_rowid();
        block_on(tx.commit())?;
        Ok(id)
    }
    pub fn imported(&mut self, receipt: &str) -> Result<bool, Error> {
        Ok(block_on(sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM coverage WHERE receipt=?1 AND published=1) AS "exists!: bool""#, receipt).fetch_one(&mut self.db))?)
    }
    pub fn can_publish(&mut self, receipt: &spool::Receipt) -> Result<bool, Error> {
        Ok(self.generation()? == receipt.generation || block_on(sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM pins WHERE channel=?1 AND start<?2 AND end>?3) AS "exists!: bool""#, receipt.channel, receipt.range.end, receipt.range.start).fetch_one(&mut self.db))?)
    }
    pub fn discard_staged(
        &mut self,
        _lease: &ProviderLease,
        keep: Option<&str>,
    ) -> Result<(), Error> {
        block_on(
            sqlx::query!(
                "DELETE FROM coverage WHERE published=0 AND (?1 IS NULL OR receipt<>?1)",
                keep
            )
            .execute(&mut self.db),
        )?;
        Ok(())
    }
    pub fn import_batch(
        &mut self,
        id: i64,
        range: Interval,
        comments: &mut Vec<Comment>,
    ) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
        let mut bytes = 0;
        {
            for comment in comments.iter() {
                let Some(time) = comment
                    .timestamp_micros
                    .and_then(|t| i64::try_from(t).ok())
                    .filter(|t| range.contains(t / 1_000_000))
                else {
                    continue;
                };
                let payload = serde_json::to_vec(&StoredComment::from(comment.clone()))?;
                bytes += payload.len() as i64 + ROW_CHARGE;
                block_on(
                    sqlx::query!(
                        "INSERT INTO comments(coverage,time,payload) VALUES(?1,?2,?3)",
                        id,
                        time,
                        payload
                    )
                    .execute(&mut *tx),
                )?;
            }
        }
        block_on(
            sqlx::query!("UPDATE coverage SET bytes=bytes+?1 WHERE id=?2", bytes, id)
                .execute(&mut *tx),
        )?;
        block_on(tx.commit())?;
        comments.clear();
        Ok(())
    }
    pub fn publish(&mut self, id: i64, receipt: &spool::Receipt) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin_with("BEGIN IMMEDIATE"))?;
        let generation = block_on(
            sqlx::query_scalar!("SELECT generation FROM provider WHERE id=1").fetch_one(&mut *tx),
        )?;
        let pinned = block_on(sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM pins WHERE channel=?1 AND start<?2 AND end>?3) AS "exists!: bool""#, receipt.channel, receipt.range.end, receipt.range.start).fetch_one(&mut *tx))?;
        if receipt.generation != generation && !pinned {
            block_on(sqlx::query!("DELETE FROM coverage WHERE id=?1", id).execute(&mut *tx))?;
            block_on(tx.commit())?;
            return Ok(());
        }
        let range = receipt.range;
        // Responses overlap intentionally. Preserve the maximum multiplicity
        // across snapshots, including genuine identical posts in one response.
        // SQL's sort spills to disk rather than buffering a programme in RAM.
        block_on(
            sqlx::query_file!(
                "src/cache/store/queries/merge_comments.sql",
                receipt.channel,
                range.start * 1_000_000,
                range.end * 1_000_000,
                id
            )
            .execute(&mut *tx),
        )?;
        let overlap = block_on(
            sqlx::query_file!(
                "src/cache/store/queries/overlapping_coverage.sql",
                receipt.channel,
                range.end,
                range.start,
                id
            )
            .fetch_all(&mut *tx),
        )?;
        for row in overlap {
            let (old, start, end, fetched, used, settled, target) = (
                row.id,
                row.start,
                row.end,
                row.fetched,
                row.last_used,
                row.settled,
                row.target_key,
            );
            for remain in [
                Interval::new(start, range.start.min(end)),
                Interval::new(range.end.max(start), end),
            ]
            .into_iter()
            .flatten()
            {
                let inserted = block_on(sqlx::query!("INSERT INTO coverage(channel,start,end,fetched,last_used,published,bytes,settled,target_key) VALUES(?1,?2,?3,?4,?5,1,0,?6,?7)", receipt.channel, remain.start, remain.end, fetched, used, settled, target).execute(&mut *tx))?;
                let kept = inserted.last_insert_rowid();
                block_on(sqlx::query!("UPDATE comments SET coverage=?1 WHERE coverage=?2 AND time>=?3 AND time<?4", kept, old, remain.start * 1_000_000, remain.end * 1_000_000).execute(&mut *tx))?;
                block_on(sqlx::query!("UPDATE coverage SET bytes=?3+COALESCE((SELECT SUM(length(payload)+?1) FROM comments WHERE coverage=?2),0) WHERE id=?2", ROW_CHARGE, kept, COVERAGE_CHARGE).execute(&mut *tx))?;
            }
            block_on(sqlx::query!("DELETE FROM coverage WHERE id=?1", old).execute(&mut *tx))?;
        }
        let settled = receipt.fetched
            >= receipt
                .plan
                .as_ref()
                .map_or(range.end, |p| p.target.range.end)
                .saturating_add(SETTLED_SECONDS);
        let target_key = receipt.plan.as_ref().map(|p| p.target.key());
        block_on(
            sqlx::query_file!(
                "src/cache/store/queries/publish_coverage.sql",
                id,
                settled,
                target_key,
                ROW_CHARGE,
                COVERAGE_CHARGE
            )
            .execute(&mut *tx),
        )?;
        if let Some(plan) = &receipt.plan {
            block_on(sqlx::query!("UPDATE targets SET completed_refresh=MAX(completed_refresh,?2),failures=0,retry_at=0,stopped=0,failure=NULL WHERE key=?1", plan.target.key(), plan.refresh).execute(&mut *tx))?;
        }
        block_on(
            sqlx::query!(
                "UPDATE provider SET failures=0,wait_until=0,revision=revision+1 WHERE id=1"
            )
            .execute(&mut *tx),
        )?;
        block_on(tx.commit())?;
        Ok(())
    }
    pub fn revision(&mut self) -> Result<i64, Error> {
        Ok(block_on(
            sqlx::query_scalar!("SELECT revision FROM provider WHERE id=1").fetch_one(&mut self.db),
        )?)
    }
    pub fn read(&mut self, owner: &str, channel: u16, view: &View) -> Result<Vec<Record>, Error> {
        let start = view.interval.start * 1_000_000;
        let end = view.interval.end * 1_000_000;
        let limit = WORKING_COMMENTS as i64;
        let query = sqlx::query_file!(
            "src/cache/store/queries/read_window.sql",
            channel,
            start,
            end,
            owner,
            view.clock_key,
            limit
        );
        let mut rows = query.fetch(&mut self.db);
        let mut records = Vec::new();
        let mut bytes = 0;
        // Stream rows to retain the working-set budget, including large payloads.
        // The connection's one-row buffer bounds read-ahead in SQLx's worker.
        while let Some(row) = block_on(rows.next()) {
            let row = row?;
            bytes += row.payload.len() + std::mem::size_of::<Record>();
            if bytes > WORKING_BYTES {
                break;
            }
            let stored: StoredComment = serde_json::from_slice(&row.payload)?;
            records.push(Record {
                id: (row.id as u64) * 2 + u64::from(row.origin),
                comment: stored.into(),
                own: row.own,
                origin: if row.origin {
                    RecordOrigin::Live
                } else {
                    RecordOrigin::Archive
                },
                media_ms: row.media_ms,
            });
        }
        // Reading a published window never acquires a write lock. Pinning and
        // maintenance update usage independently of this read.
        Ok(records)
    }
    pub fn cleanup(&mut self, now: i64, clear: bool) -> Result<(), Error> {
        let mut tx = block_on(self.db.begin())?;
        touch_pinned(&mut tx, now)?;
        if clear {
            block_on(
                sqlx::query!(
                    "UPDATE provider SET generation=generation+1,revision=revision+1 WHERE id=1"
                )
                .execute(&mut *tx),
            )?;
        }
        let candidates = block_on(
            sqlx::query_file!("src/cache/store/queries/eviction_candidates.sql")
                .fetch_all(&mut *tx),
        )?;
        let mut bytes = block_on(
            sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(bytes),0) AS "bytes!: i64" FROM coverage WHERE published=1"#
            )
            .fetch_one(&mut *tx),
        )?;
        for row in candidates {
            let (id, charge) = (row.key, row.bytes);
            if clear || bytes > self.cache_bytes {
                block_on(sqlx::query!("DELETE FROM coverage WHERE published=1 AND COALESCE(target_key,'legacy:'||id)=?1", id).execute(&mut *tx))?;
                bytes -= charge;
                block_on(
                    sqlx::query!("UPDATE provider SET revision=revision+1 WHERE id=1")
                        .execute(&mut *tx),
                )?;
            }
        }
        block_on(tx.commit())?;
        block_on(
            sqlx::raw_sql("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE);")
                .execute(&mut self.db),
        )?;
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

fn touch_pinned(db: &mut SqliteConnection, now: i64) -> Result<(), Error> {
    block_on(sqlx::query_file!("src/cache/store/queries/touch_pinned.sql", now).execute(&mut *db))?;
    Ok(())
}
