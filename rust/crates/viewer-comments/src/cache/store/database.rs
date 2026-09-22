//! SQLite ownership stays on the comment-store worker. SQLx's SQLite driver is
//! runtime-independent; this blocking boundary never runs on the Qt thread and
//! needs neither a connection pool nor a second Tokio runtime.
use super::{COVERAGE_CHARGE, Error, SETTLED_SECONDS};
use futures_lite::future::block_on;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{path::Path, time::Duration};

const SCHEMA_VERSION: i64 = 2;
const APPLICATION_ID: i64 = 0x4e434d54;
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);
const READ_AHEAD_ROWS: usize = 1;

pub(super) fn open(path: &Path) -> Result<SqliteConnection, Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .busy_timeout(BUSY_TIMEOUT)
        .row_buffer_size(READ_AHEAD_ROWS);
    let mut db = block_on(SqliteConnection::connect_with(&options))?;
    let version = block_on(sqlx::query_scalar::<_, i64>("PRAGMA user_version").fetch_one(&mut db))?;
    let app = block_on(sqlx::query_scalar::<_, i64>("PRAGMA application_id").fetch_one(&mut db))?;
    if !(0..=SCHEMA_VERSION).contains(&version) || (app != 0 && app != APPLICATION_ID) {
        return Err(Error::Format("unsupported database version".into()));
    }
    if version == 0 {
        block_on(sqlx::raw_sql("PRAGMA auto_vacuum=INCREMENTAL;").execute(&mut db))?;
    }
    block_on(
        sqlx::raw_sql(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
         PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=FILE;",
        )
        .execute(&mut db),
    )?;
    if version < SCHEMA_VERSION {
        let mut tx = block_on(db.begin_with("BEGIN IMMEDIATE"))?;
        // Another process may have migrated while we waited for the write lock.
        let current =
            block_on(sqlx::query_scalar::<_, i64>("PRAGMA user_version").fetch_one(&mut *tx))?;
        if current < SCHEMA_VERSION {
            block_on(sqlx::raw_sql(include_str!("../schema.sql")).execute(&mut *tx))?;
            block_on(sqlx::raw_sql(include_str!("../schema-v2.sql")).execute(&mut *tx))?;
            block_on(
                sqlx::query!(
                    "UPDATE coverage SET settled=(fetched>=end+?1),bytes=bytes+?2",
                    SETTLED_SECONDS,
                    COVERAGE_CHARGE
                )
                .execute(&mut *tx),
            )?;
            // PRAGMA assignments cannot bind parameters. Only internal constants
            // are interpolated; application data always uses checked queries.
            block_on(
                sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                    "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={SCHEMA_VERSION};"
                )))
                .execute(&mut *tx),
            )?;
        }
        block_on(tx.commit())?;
    }
    Ok(db)
}
